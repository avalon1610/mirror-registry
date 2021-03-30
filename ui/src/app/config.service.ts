import { HttpClient } from '@angular/common/http';
import { Injectable } from '@angular/core';
import { Observable, throwError } from 'rxjs';
import { catchError, map, shareReplay } from 'rxjs/operators';
import { UtilsService } from './utils.service';

export class Config {
  git: GitConfig;
  registry: RegistryConfig;
  crates: CratesConfig;
  database: DatabaseConfig;
  inited: boolean;
  busy: boolean;
  salt: string;
}

export class GitConfig {
  index_path: string;
  working_path: string;
  upstream_url: string;
}

export class CratesConfig {
  storage_path: string;
  upstream_url: string;
}

export class RegistryConfig {
  address: string;
  interval: string;
  can_create_account: boolean;
  ldap: Ldap;
}

export class Ldap {
  hostname: string;
  base_dn: string;
  domain: string;
  username: string;
  password: string;
}

export class DatabaseConfig {
  url: string;
}

@Injectable({
  providedIn: 'root'
})
export class ConfigService {
  current: Config = {
    git: { index_path: '', working_path: '', upstream_url: '' },
    registry: {
      address: '', interval: '', can_create_account: false,
      ldap: { hostname: '', base_dn: '', username: '', password: '', domain: '' },
    },
    crates: { storage_path: '', upstream_url: '' },
    database: { url: '' }, inited: false, busy: false, salt: ''
  };

  constructor(private http: HttpClient, private utils: UtilsService) { }

  refresh(): Observable<any> {
    return this.http.get<Config>(`${this.utils.prefix()}/web_api/config`).pipe(
      map(cfg => {
        this.current = cfg;
      }),
      shareReplay(1)
    );
  }

  save(): Observable<any> {
    this.current.busy = true;
    return this.http.post(`${this.utils.prefix()}/web_api/config`, this.current).pipe(
      catchError(err => {
        this.current.busy = false;
        return throwError(err);
      }),
      map(_ => this.current.busy = false),
      shareReplay(1)
    );
  }
}
