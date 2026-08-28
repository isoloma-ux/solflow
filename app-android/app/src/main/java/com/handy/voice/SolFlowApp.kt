package com.handy.voice

import android.app.Application

/**
 * Тему нужно выставить раньше, чем откроется первый экран, — иначе
 * приложение мигает системной темой поверх выбранной пользователем.
 */
class SolFlowApp : Application() {

    override fun onCreate() {
        super.onCreate()
        SettingsActivity.applyTheme(AppPrefs.theme(this))
    }
}
