import { HttpClient } from '@angular/common/http';
import { Injectable } from '@angular/core';
import { ActivatedRouteSnapshot, CanActivate, Router, RouterStateSnapshot } from '@angular/router';
import { Observable, Subject, throwError } from 'rxjs';
import { catchError } from 'rxjs/operators';
import { UtilsService } from './utils.service';

class User {
  username: string;
  role: string;
  token: string;
  type: string;
}

@Injectable({
  providedIn: 'root'
})
export class AuthService {
  user: User;
  constructor(private http: HttpClient, private utils: UtilsService, private router: Router) {
  }

  ldap_login(): Observable<[boolean, string]> {
    const result = new Subject<[boolean, string]>();
    this.http.get<User>(`${this.utils.prefix()}/auth/ldap_login`).pipe(catchError(err => {
      let errMsg: string;
      if (err.error instanceof ErrorEvent) {
        errMsg = err.error.message;
      } else {
        errMsg = err.error;
      }

      result.next([false, errMsg]);
      return throwError(`login failed: ${errMsg}`);
    })).subscribe((user: User) => {
      this.user = user;
      result.next([true, '']);
    });

    return result;
  }

  modify(data: any): Observable<any> {
    return this.http.post(`${this.utils.prefix()}/auth/modify`, data);
  }

  login(): Observable<[boolean, string]> {
    const result = new Subject<[boolean, string]>();
    this.http.get<User>(`${this.utils.prefix()}/auth/login`).pipe(catchError(err => {
      let errMsg: string;
      if (err.error instanceof ErrorEvent) {
        errMsg = err.error.message;
      } else {
        errMsg = err.error;
      }

      result.next([false, errMsg]);
      return throwError(`login failed: ${errMsg}`);
    })).subscribe((user: User) => {
      this.user = user;
      result.next([true, '']);
    });

    return result;
  }

  logout(): void {
    this.http.get(`${this.utils.prefix()}/auth/logout`).subscribe(_ => {
      this.user = null;
      this.router.navigateByUrl('/');
    });
  }

  create(data: any): Observable<any> {
    return this.http.post(`${this.utils.prefix()}/auth/create`, data);
  }
}

@Injectable({
  providedIn: 'root'
})
export class AuthGuard implements CanActivate {
  constructor(private auth: AuthService, private router: Router) {

  }

  canActivate(next: ActivatedRouteSnapshot, state: RouterStateSnapshot): boolean {
    if (this.auth.user) {
      let type = this.auth.user.role;
      if (type == "Root" || type == 'Admin')
        return true;
    }

    this.router.navigate(['/status']);
    return false;
  }
}