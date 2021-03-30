import { HttpErrorResponse } from '@angular/common/http';
import { Component, OnInit } from '@angular/core';
import { FormBuilder, FormControl, FormGroup, Validators } from '@angular/forms';
import { NzMessageService } from 'ng-zorro-antd/message';
import { Observable, throwError } from 'rxjs';
import { catchError } from 'rxjs/operators';
import { AuthService } from './auth.service';
import { ConfigService } from './config.service';
import { Md5 } from 'ts-md5/dist/md5';

@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.css']
})
export class AppComponent implements OnInit {
  title = 'mirror registry';
  isVisible = false;
  isUpdateModal = false;
  isOkLoading = false;
  createForm!: FormGroup;
  constructor(public auth: AuthService, private msg: NzMessageService, private fb: FormBuilder, public config: ConfigService) { }

  ngOnInit(): void {
    this.createForm = this.fb.group({
      username: [null, [Validators.required]],
      password: [null, [Validators.required]],
      checkPassword: [null, [Validators.required, this.confirmationValidator]],
      email: [null, [Validators.email]]
    })

    this.config.refresh().subscribe(_ => { });
  }


  encodePassword(user: string, pwd: string): string {
    return Md5.hashAsciiStr(`${user}:${this.config.current.salt}:${pwd}`) as string;
  }

  confirmationValidator = (control: FormControl): { [s: string]: boolean } => {
    if (!control.value) {
      return { required: true };
    } else if (control.value !== this.createForm.controls.password.value) {
      return { confirm: true, error: true }
    }

    return {};
  }

  updateConfirmValidator(): void {
    Promise.resolve().then(() => this.createForm.controls.checkPassword.updateValueAndValidity());
  }

  ldap_login(): void {
    this.auth.ldap_login().subscribe(([ok, err]) => {
      if (ok) {
        this.msg.info('ldap login ok');
      } else {
        this.msg.error(`ldap login error: ${err}`);
      }
    });
  }

  login(): void {
    this.auth.login().subscribe(([ok, err]) => {
      if (ok) {
        this.msg.info('login ok');
      } else {
        this.msg.error(`login error: ${err}`);
      }
    });
  }

  logout(): void {
    this.auth.logout();
  }

  showModal(isUpdate: boolean) {
    this.isVisible = true;
    if (isUpdate) {
      this.isUpdateModal = isUpdate;
      this.createForm.controls.username.setValue(this.auth.user.username);
    }
  }

  handleOk(): void {
    for (const i in this.createForm.controls) {
      this.createForm.controls[i].markAsDirty();
      this.createForm.controls[i].updateValueAndValidity();
    }

    this.isOkLoading = true;
    let user = this.createForm.controls.username.value;
    let pwd = this.createForm.controls.password.value;
    let encodedPwd = this.encodePassword(user, pwd);
    let email = this.createForm.controls.email.value;
    let op: Observable<any>;
    if (this.isUpdateModal) {
      op = this.auth.modify({ username: user, password: encodedPwd, email: email });
    } else {
      op = this.auth.create({ username: user, password: encodedPwd, email: email });
    }
    op.pipe(catchError(e => {
      this.showError(e);
      this.createForm.reset();
      this.isOkLoading = false;
      return throwError(e);
    })).subscribe(() => {
      this.isOkLoading = false;
      this.isVisible = false;
      this.createForm.reset();
      if (this.isUpdateModal)
        this.msg.info("update account ok");
      else
        this.msg.info("create account ok");
    });
  }

  showError(err: HttpErrorResponse) {
    if (typeof err.error === "string") {
      this.msg.error(err.error);
    } else {
      this.msg.error(err.message);
    }
  }

  handleCancel(): void {
    this.createForm.reset();
    this.isVisible = false;
  }
}
