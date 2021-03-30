import { Injectable, isDevMode } from '@angular/core';

@Injectable({
  providedIn: 'root'
})
export class UtilsService {

  constructor() { }

  prefix(): string {
    return isDevMode() ? 'http://localhost:55555' : '';
  }
}
