use crate::{config::Config, Server};
use anyhow::{anyhow, Context, Result};
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, StreamExt};
use log::{debug, error, info};
use serde::Serialize;
use spa_server::re_export::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    get,
    http::StatusCode,
    post, web, HttpMessage, HttpRequest, HttpResponse,
};
use std::{
    convert::{TryFrom, TryInto},
    fs::{self, create_dir_all},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};
use tokio::{
    sync::{Mutex, RwLock},
    time,
};

pub struct Git {
    http_backend: PathBuf,
    config: Arc<RwLock<Config>>,

    #[doc(hidden)]
    pub inited: Mutex<bool>,

    #[doc(hidden)]
    pub busy: Mutex<bool>,
}

struct GitCmd<'a> {
    cmd: &'static str,
    dir: Option<&'a Path>,
}
#[cfg(test)]
mod test {
    use super::convert_args;

    #[test]
    fn test_convert_args() {
        let test_str = "commit -m \"change config.json to mirror\"";
        let result = vec!["commit", "-m", "\"change config.json to mirror\""];
        assert_eq!(result, convert_args(test_str).unwrap())
    }
}

fn convert_args(args: &str) -> anyhow::Result<Vec<String>> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let mut args_fixed = Vec::new();
    let mut quote_args = String::new();
    let mut in_quote = false;
    for index in 0..args.len() {
        if args[index].starts_with('"') {
            quote_args = args[index].to_string();
            in_quote = true;
            continue;
        }

        if in_quote {
            quote_args.push(' ');
            quote_args.push_str(args[index]);

            if args[index].ends_with('"') {
                args_fixed.push(quote_args.clone());
                in_quote = false;
            }
        } else {
            args_fixed.push(args[index].to_string());
        }
    }

    if in_quote {
        return Err(anyhow!("found single quote args, abort"));
    }

    Ok(args_fixed)
}

impl<'a> GitCmd<'a> {
    fn dir<'b: 'a>(dir: &'b Path) -> Self {
        GitCmd {
            cmd: "git",
            dir: Some(dir),
        }
    }

    #[allow(dead_code)]
    fn new() -> Self {
        GitCmd {
            cmd: "git",
            dir: None,
        }
    }

    #[allow(dead_code)]
    fn cmd(cmd: &'static str) -> Self {
        GitCmd { cmd, dir: None }
    }

    fn run(&self, args: impl AsRef<str>) -> anyhow::Result<String> {
        debug!("command input : git {}", args.as_ref());

        let mut cmd = Command::new(self.cmd);
        if let Some(dir) = self.dir {
            cmd.current_dir(dir);
        }

        let args = convert_args(args.as_ref())?;
        debug!("args: {:?}", args);
        let output = cmd
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()
            .context("run git command failed")?;

        if output.status.success() {
            let o = String::from_utf8_lossy(&*output.stdout);
            let o = o.trim();
            debug!("command output: {}", o);
            Ok(o.to_string())
        } else {
            Err(
                anyhow!("{}", String::from_utf8_lossy(&*output.stderr).to_string())
                    .context("git command error"),
            )
        }
    }
}

#[derive(Serialize)]
struct IndexConfig {
    dl: String,
    api: String,
}

impl Git {
    pub async fn new(cfg: Arc<RwLock<Config>>) -> anyhow::Result<Arc<Self>> {
        let http_backend = find_git_http_backend()?;
        let git = Arc::new(Git {
            http_backend,
            config: cfg.clone(),
            inited: Mutex::new(false),
            busy: Mutex::new(false),
        });

        let git_clone = git.clone();
        let _ = tokio::spawn(async move {
            let mut interval = cfg.read().await.registry.interval;
            debug!("schedule start running, next: {}s", interval.as_secs());
            loop {
                time::sleep(interval).await;
                interval = cfg.read().await.registry.interval;

                if *git_clone.inited.lock().await {
                    info!(
                        "sync upstream by schedule now, next: {}s",
                        interval.as_secs()
                    );
                    if let Err(e) = git_clone.sync_upstream().await {
                        error!("sync upstream by schedule failed: {:?}", e);
                    } else {
                        if let Err(e) = git_clone.sync_upstream().await {
                            error!("sync index by schedule failed: {:?}", e);
                        }
                    }
                }
            }
        });

        Ok(git)
    }

    pub async fn initialize(&self) -> Result<()> {
        self.init_repo().await?;
        self.sync_upstream().await?;
        self.modify_config_json().await?;
        self.sync_index().await?;

        Ok(())
    }

    pub async fn sync_index(&self) -> Result<()> {
        GitCmd::dir(&self.config.read().await.git.working_path)
            .run("push origin master")
            .context("sync with index failed")?;
        Ok(())
    }

    pub async fn commit(&self, message: impl AsRef<str>) -> anyhow::Result<()> {
        let working_path = &self.config.read().await.git.working_path;
        GitCmd::dir(working_path).run("add .")?;
        GitCmd::dir(working_path).run(format!("commit -m \"{}\"", message.as_ref()))?;
        Ok(())
    }

    async fn modify_config_json(&self) -> Result<()> {
        let cfg = self.config.read().await;
        let config_json_path = cfg.git.working_path.join("config.json");
        let content =
            fs::read_to_string(config_json_path).context("can not read config.json content")?;

        let config_json = serde_json::to_string_pretty(&IndexConfig {
            dl: format!("{}/api/v1/crates", cfg.registry.address),
            api: cfg.registry.address.clone(),
        })
        .context("generate config.json failed")?;
        if content != config_json {
            fs::write(cfg.git.working_path.join("config.json"), config_json)
                .context("write config.json failed")?;
            GitCmd::dir(&cfg.git.working_path)
                .run("add .")
                .context("modify config json, run git add . failed")?;
            GitCmd::dir(&cfg.git.working_path)
                .run("commit -m \"change config.json to mirror\"")
                .context("modify config json, run git commit failed")?;
        }

        Ok(())
    }

    async fn sync_upstream(&self) -> anyhow::Result<()> {
        // here --progress flag must be set
        GitCmd::dir(&self.config.read().await.git.working_path)
            .run("pull --progress upstream master")
            .context("sync with upstream failed")?;

        Ok(())
    }

    async fn init_repo(&self) -> Result<()> {
        let cfg = self.config.read().await;
        let mut bare_repo_inited = false;
        let mut work_tree_inited = false;
        if !Path::new(&cfg.git.index_path).exists() {
            create_dir_all(&cfg.git.index_path)?;
        }

        if let Ok(r) = GitCmd::dir(&cfg.git.index_path).run("rev-parse --is-bare-repository") {
            if r == "true" {
                bare_repo_inited = true;
                debug!("{:?} is a bare repo already", cfg.git.index_path);
            }
        }

        if !bare_repo_inited {
            info!("git init --bare for {:?}", cfg.git.index_path);

            if let Err(e) = GitCmd::dir(&cfg.git.index_path).run("init --bare") {
                return Err(e.context("git init --bare failed"));
            }
        }

        if !Path::new(&cfg.git.working_path).exists() {
            create_dir_all(&cfg.git.working_path)?;
        }

        if let Ok(r) = GitCmd::dir(&cfg.git.working_path).run("rev-parse --is-inside-work-tree") {
            if r == "true" {
                debug!("{:?} is a work tree already", cfg.git.working_path);
                work_tree_inited = true;
            }
        }

        if !work_tree_inited {
            info!("git clone for {:?}", cfg.git.working_path);

            if let Err(e) = GitCmd::dir(&cfg.git.working_path.parent().ok_or(anyhow!(
                "work tree {:?} has no parent",
                cfg.git.working_path
            ))?)
            .run(format!(
                "clone {} {}",
                cfg.git.index_path.to_string_lossy().to_string(),
                cfg.git
                    .working_path
                    .file_name()
                    .ok_or(anyhow!("work tree {:?} has no filename"))?
                    .to_string_lossy()
                    .to_string()
            )) {
                return Err(e.context("git clone failed"));
            }

            info!("git remote add upstream {}", cfg.git.upstream_url);

            if let Err(e) = GitCmd::dir(&cfg.git.working_path)
                .run(format!("remote add upstream {}", cfg.git.upstream_url))
            {
                return Err(e.context("git add remote failed"));
            }
        }

        Ok(())
    }
}

fn find_git_http_backend() -> Result<PathBuf> {
    if let Err(e) = Command::new("git").stdout(Stdio::null()).spawn() {
        if ErrorKind::NotFound == e.kind() {
            panic!("git not found, you need install git first");
        }
    }

    let output = Command::new("which").arg("git").output()?;
    if output.status.success() {
        let git_path = std::str::from_utf8(&*output.stdout)?.trim();
        let backend_path1 =
            Path::new(&git_path.replace("bin/git", "lib")).join("git-core/git-http-backend");
        let backend_path2 =
            Path::new(&git_path.replace("bin/git", "libexec")).join("git-core/git-http-backend");

        let http_backend_path = if backend_path1.exists() {
            backend_path1
        } else if backend_path2.exists() {
            backend_path2
        } else {
            panic!("can not found git-http-backend, upgrade your git");
        };

        info!("git-http-backend path: {:?}", http_backend_path);
        return Ok(http_backend_path);
    }

    panic!("which command failed! on windows? not implement yet");
}

#[post("/crates.io-index/.*")]
pub(crate) async fn http_backend_post(
    req: HttpRequest,
    body: web::Payload,
    data: web::Data<Server>,
) -> spa_server::re_export::Result<HttpResponse> {
    http_backend(req, Some(body), data).await
}

#[get("/crates.io-index/.*")]
pub(crate) async fn http_backend_get(
    req: HttpRequest,
    data: web::Data<Server>,
) -> spa_server::re_export::Result<HttpResponse> {
    http_backend(req, None, data).await
}

async fn http_backend(
    req: HttpRequest,
    body: Option<web::Payload>,
    data: web::Data<Server>,
) -> spa_server::re_export::Result<HttpResponse> {
    if !*data.git.inited.lock().await {
        return Err(ErrorBadRequest("System not initialized"));
    }

    debug!("git req:{:?}", req);
    let request_method = req.method().to_string();
    let mut path_info = req
        .uri()
        .to_string()
        .replace("/registry/crates.io-index", "");
    if let Some(i) = path_info.find('?') {
        path_info = path_info.split_at(i).0.to_string();
    }

    let mut child = Command::new(&data.git.http_backend)
        .env("REQUEST_METHOD", request_method)
        .env("GIT_PROJECT_ROOT", &data.config.read().await.git.index_path)
        .env("PATH_INFO", path_info)
        .env("REMOTE_USER", "raven")
        .env("REMOTE_ADDR", "dahua")
        .env("QUERY_STRING", req.query_string())
        .env("CONTENT_TYPE", req.content_type())
        .env("GIT_HTTP_EXPORT_ALL", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut body) = body {
        let child_stdin = child
            .stdin
            .as_mut()
            .ok_or(ErrorInternalServerError("can not open cgi stdin"))?;
        while let Some(b) = body.next().await {
            let b = b?;
            child_stdin.write_all(&*b)?;
        }
        drop(child_stdin);
    }

    let o = child.wait_with_output()?;
    let err = String::from_utf8_lossy(&*o.stderr);
    if !err.is_empty() {
        error!("cgi error: {}", err.trim());
    }

    let mut reader = BufReader::new(&*o.stdout);
    let mut status_code = 200u16;
    let mut headers = Vec::new();
    let mut line = String::new();
    while reader.read_line(&mut line).await? != 0 {
        let l = line.trim();
        if l.is_empty() {
            // next is body part
            break;
        }

        if l.starts_with("Status:") {
            status_code = l.split(':').collect::<Vec<&str>>()[1]
                .split(' ')
                .collect::<Vec<&str>>()[1]
                .parse()
                .map_err(|e| ErrorInternalServerError(format!("parse cgi status code: {:?}", e)))?;
            continue;
        }

        if let Some(i) = l.find(':') {
            let (k, v) = l.split_at(i + 1);
            headers.push((k.trim_end_matches(':').to_string(), v.trim().to_string()));
        } else {
            return Err(ErrorInternalServerError(format!(
                "unknown part of cgi response: {:?}",
                l
            )));
        }

        line.clear();
    }

    let mut body = Vec::new();
    reader.read_to_end(&mut body).await?;
    debug!("cgi return:\n{:?}\nbody size:{}", headers, body.len());

    let cgi_resp = CgiResponse {
        code: status_code,
        headers: headers
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        body,
    };
    Ok(cgi_resp.try_into()?)
}

struct CgiResponse {
    code: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl TryFrom<CgiResponse> for HttpResponse {
    type Error = spa_server::re_export::Error;

    fn try_from(cr: CgiResponse) -> Result<Self, Self::Error> {
        let mut builder =
            HttpResponse::build(StatusCode::from_u16(cr.code).map_err(|e| {
                ErrorInternalServerError(format!("invalid cgi status code: {:?}", e))
            })?);
        for header in cr.headers {
            builder.append_header(header);
        }

        if !cr.body.is_empty() {
            Ok(builder.body(cr.body))
        } else {
            Ok(builder.finish())
        }
    }
}
