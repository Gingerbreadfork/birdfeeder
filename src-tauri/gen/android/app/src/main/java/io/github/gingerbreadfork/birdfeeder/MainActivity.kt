package io.github.gingerbreadfork.birdfeeder

import android.content.Intent
import android.content.res.Configuration
import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.WebViewCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    makeRoom()
    checkIfAsked(intent)
  }

  // Drawn edge to edge, the page is not resized for the keyboard, so the room is made by hand.
  // A web view before 136 cannot tell the page where the system bars are, so they get room too.
  private fun makeRoom() {
    val webView = WebViewCompat.getCurrentWebViewPackage(this)?.versionName
    val major = webView?.substringBefore('.')?.toIntOrNull() ?: 0
    val bars = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
    val padBars = major < 136
    ViewCompat.setOnApplyWindowInsetsListener(findViewById<View>(android.R.id.content)) { view, insets ->
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      val taken = if (padBars) insets.getInsets(bars) else Insets.NONE
      view.setPadding(taken.left, taken.top, taken.right, maxOf(keyboard.bottom, taken.bottom))
      val passed = WindowInsetsCompat.Builder(insets).setInsets(WindowInsetsCompat.Type.ime(), Insets.NONE)
      if (padBars) passed.setInsets(bars, Insets.NONE)
      passed.build()
    }
  }

  // Leaving the screen is when background checks start to matter, and the setting is settled by then.
  override fun onStop() {
    super.onStop()
    CheckJob.schedule(this)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    checkIfAsked(intent)
  }

  // A debug build runs the background check ten seconds after being started with
  // `adb shell am start -n <package>/.MainActivity --ez check-now true`.
  private fun checkIfAsked(intent: Intent?) {
    if (BuildConfig.DEBUG && intent?.getBooleanExtra("check-now", false) == true) {
      CheckJob.runSoon(this)
    }
  }

  // Dark mode changes without the activity starting over, so the bar icons follow by hand.
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    enableEdgeToEdge()
  }
}
