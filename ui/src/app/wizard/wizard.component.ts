import { HttpClient, HttpErrorResponse } from '@angular/common/http';
import { ChangeDetectorRef, Component, OnInit, TemplateRef, ViewChild } from '@angular/core';
import { NzMessageService } from 'ng-zorro-antd/message';
import { NzNotificationService } from 'ng-zorro-antd/notification';
import { throwError } from 'rxjs';
import { catchError } from 'rxjs/operators';
import { ConfigService } from '../config.service';
import { UtilsService } from '../utils.service';

@Component({
  selector: 'app-wizard',
  templateUrl: './wizard.component.html',
  styleUrls: ['./wizard.component.css']
})
export class WizardComponent implements OnInit {
  @ViewChild('step1') step1: TemplateRef<any>;
  @ViewChild('step2') step2: TemplateRef<any>;
  @ViewChild('step3') step3: TemplateRef<any>;
  @ViewChild('step4') step4: TemplateRef<any>;
  current_step = 0;
  cargo_msg: string;
  enable_ldap = false;

  constructor(private http: HttpClient, private message: NzMessageService, private cdr: ChangeDetectorRef,
    private notify: NzNotificationService, private utils: UtilsService, public config: ConfigService) {

  }

  ngAfterViewInit() {
    this.cdr.detectChanges();
  }

  ngOnInit(): void {
    this.refresh();
  }

  onIndexChange(index: number) {
    this.current_step = index;
  }

  pre(): void {
    this.current_step -= 1;
  }

  next() {
    this.current_step += 1;
  }

  refresh() {
    this.config.refresh().pipe(catchError(err => {
      this.showError(err);
      return throwError(err);
    })).subscribe(_ => {
      this.enable_ldap = this.config.current.registry.ldap != null;
    });
  }

  toggleLdap() {
    if (!this.enable_ldap) {
      this.config.current.registry.ldap = null;
    }
    else if (this.enable_ldap && this.config.current.registry.ldap == null) {
      this.config.current.registry.ldap = { hostname: '', base_dn: '', username: '', password: '', domain: '' };
    }
  }

  save() {
    this.config.save().pipe(catchError(err => {
      this.showError(err);
      return throwError(err);
    })).subscribe(_ => {
      this.notify.success('Config Saved', 'new config has beed merged');
    });
  }

  init() {
    this.config.save().pipe(catchError(err => {
      this.showError(err);
      return throwError(err);
    })).subscribe(_ => {
      this.config.current.busy = true;
      this.http.get(`${this.utils.prefix()}/web_api/init`).pipe(catchError(err => {
        this.showError(err);
        return throwError(err);
      })).subscribe(_ => {
        this.refresh();
        this.notify.success('Initialize OK', 'now mirror registry has been configured successfully, goto home page for more info');
      });
    });
  }

  get changeContent() {
    switch (this.current_step) {
      case 0: {
        return this.step1;
      }
      case 1: {
        return this.step2;
      }
      case 2: {
        return this.step3;
      }
      case 3: {
        return this.step4;
      }
    }
  }

  showError(err: HttpErrorResponse) {
    if (typeof err.error === "string") {
      this.message.error(err.error);
    } else {
      this.message.error(err.message);
    }
  }
}
