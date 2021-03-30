import { NgModule } from '@angular/core';
import { BrowserModule } from '@angular/platform-browser';

import { AppComponent } from './app.component';
import { NZ_I18N } from 'ng-zorro-antd/i18n';
import { en_US } from 'ng-zorro-antd/i18n';
import { formatCurrency, registerLocaleData } from '@angular/common';
import en from '@angular/common/locales/en';
import { FormsModule } from '@angular/forms';
import { HttpClientModule } from '@angular/common/http';
import { BrowserAnimationsModule } from '@angular/platform-browser/animations';
import { NzMenuModule } from 'ng-zorro-antd/menu';
import { NzLayoutModule } from 'ng-zorro-antd/layout';
import { NzIconModule } from 'ng-zorro-antd/icon';
import { IconDefinition } from '@ant-design/icons-angular';
import { SettingOutline, HomeOutline, ExclamationCircleOutline } from '@ant-design/icons-angular/icons';
import { NzStepsModule } from 'ng-zorro-antd/steps';
import { WizardComponent } from './wizard/wizard.component';
import { Routes, RouterModule } from '@angular/router';
import { NzButtonModule } from 'ng-zorro-antd/button';
import { NzGridModule } from 'ng-zorro-antd/grid';
import { NzFormModule } from 'ng-zorro-antd/form';
import { NzInputModule } from 'ng-zorro-antd/input';
import { NzToolTipModule } from 'ng-zorro-antd/tooltip';
import { NzMessageModule } from 'ng-zorro-antd/message';
import { ReactiveFormsModule } from '@angular/forms';
import { NzNotificationModule } from 'ng-zorro-antd/notification';
import { NzSpinModule } from 'ng-zorro-antd/spin';
import { NzAlertModule } from 'ng-zorro-antd/alert';
import { NzTagModule } from 'ng-zorro-antd/tag';
import { AuthGuard } from './auth.service';
import { NzModalModule } from 'ng-zorro-antd/modal';
import { NzSwitchModule } from 'ng-zorro-antd/switch';
import { HomeComponent } from './home/home.component';

const icons: IconDefinition[] = [SettingOutline, HomeOutline, ExclamationCircleOutline];

registerLocaleData(en);

const appRoutes: Routes = [
  { path: '', redirectTo: '/home', pathMatch: 'full' },
  { path: 'home', component: HomeComponent },
  { path: 'wizard', component: WizardComponent, canActivate: [AuthGuard] },
];

@NgModule({
  declarations: [
    AppComponent,
    WizardComponent,
    HomeComponent
  ],
  imports: [
    BrowserModule,
    FormsModule,
    HttpClientModule,
    BrowserAnimationsModule,
    NzMenuModule,
    NzLayoutModule,
    NzStepsModule,
    NzButtonModule,
    NzGridModule,
    NzFormModule,
    NzToolTipModule,
    NzInputModule,
    NzMessageModule,
    ReactiveFormsModule,
    NzNotificationModule,
    NzSpinModule,
    NzAlertModule,
    NzTagModule,
    NzModalModule,
    NzSwitchModule,
    RouterModule.forRoot(appRoutes, { relativeLinkResolution: 'legacy' }),
    NzIconModule.forRoot(icons)
  ],
  providers: [{ provide: NZ_I18N, useValue: en_US }, AuthGuard],
  bootstrap: [AppComponent]
})
export class AppModule { }
