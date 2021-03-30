# mirror-registry

## Mirror Registry
Mirror Registry is an [Alternate Registry](https://doc.rust-lang.org/cargo/reference/registries.html), can be used
to mirror upstream registry and serve private crates
## Features
- Mirror upstream crates.io-index
- Caching download crates from crates.io (or other upstream)
- Support full [Registry Web API](https://doc.rust-lang.org/cargo/reference/registries.html#web-api) for private crates
    * cargo login   (login for publish)
    * cargo publish (publish private crates)
    * cargo yank    (can only yank private crates)
    * cargo search  (search from upstream and private)
- User-friendly Web UI
- User registration and login
    * LDAP supported

## Prerequisites
Need git 2.0 or above installed

## Install
```rust
cargo install mirror-registry
```

## Usage
- start the registry, input super admin username and password:
```rust
./mirror-registry
```
- goto web ui (eg. http://localhost:55555), login with super admin
    * adjust the default configuration
    * initialize the system

- use it directly in cargo command:
```rust
cargo search tokio --registry=http://localhost:55555/registry/crates.io-index
```
- or setup in the ~/.cargo/config
```rust
[source.crates-io]
replace-with = "mirror"
[source.mirror]
registry = "http://localhost:55555/registry/crates.io-index"
```

## License
This project is licensed under either
[Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
or [MIT License](http://opensource.org/licenses/MIT)
