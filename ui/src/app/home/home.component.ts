import { Component, OnInit } from '@angular/core';
import { Observable } from 'rxjs';
import { map } from 'rxjs/operators';
import { AuthService } from '../auth.service';
import { Config, ConfigService } from '../config.service';

@Component({
  selector: 'app-home',
  templateUrl: './home.component.html',
  styleUrls: ['./home.component.css']
})
export class HomeComponent implements OnInit {
  cargo_msg: string;

  constructor(public auth: AuthService, public config: ConfigService) { }

  ngOnInit(): void {
    this.config.refresh().subscribe(_ => this.cargo_msg =
      `use mirror registry directly: 
      cargo search --registry=${this.config.current.registry.address}/registry/crates.io-index 
or use it as alternate registry, see https://doc.rust-lang.org/cargo/reference/registries.html
or replace cargo source to .cargo/config:
      [source.crates-io]
      replace-with = "mirror"
      [source.mirror]
      registry = "${this.config.current.registry.address}/registry/crates.io-index"`
    );
  }
}
