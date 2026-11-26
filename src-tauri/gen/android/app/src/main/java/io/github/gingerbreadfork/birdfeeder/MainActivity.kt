package io.github.gingerbreadfork.birdfeeder

import android.content.ClipData
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Bundle
import android.util.Base64
import android.util.Log
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.content.FileProvider
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.WebViewCompat
import java.io.File

class MainActivity : TauriActivity() {
  /** The theme chosen in the app: "light", "dark", or "system". */
  private var theme = "system"

  override fun onCreate(savedInstanceState: Bundle?) {
    theme = getSharedPreferences(PREFS, MODE_PRIVATE).getString(THEME, null) ?: "system"
    matchTheme()
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

  // What the page can ask of the phone.
  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(FromPage(), "BirdfeederAndroid")
  }

  private inner class FromPage {
    // The page says which theme was chosen in Settings, so the bars and window can match it.
    @JavascriptInterface
    fun setTheme(chosen: String) {
      if (chosen !in listOf("light", "dark", "system")) return
      getSharedPreferences(PREFS, MODE_PRIVATE).edit().putString(THEME, chosen).apply()
      runOnUiThread {
        theme = chosen
        matchTheme()
      }
    }

    // Opens the share sheet with a PNG picture the page has drawn, and a line of text beside it.
    // The picture waits in the cache, where the share sheet may read it and Android may clear it.
    @JavascriptInterface
    fun shareImage(name: String, base64: String, text: String): Boolean {
      return try {
        val folder = File(cacheDir, "cards")
        folder.deleteRecursively()
        folder.mkdirs()
        val plain = name.filter { it.isLetterOrDigit() || it in "-_." }.trim('.').take(80)
        val picture = File(folder, "${plain.ifEmpty { "repository" }}-card.png")
        picture.writeBytes(Base64.decode(base64, Base64.DEFAULT))
        val uri = FileProvider.getUriForFile(this@MainActivity, "$packageName.fileprovider", picture)
        val send = Intent(Intent.ACTION_SEND).apply {
          type = "image/png"
          putExtra(Intent.EXTRA_STREAM, uri)
          putExtra(Intent.EXTRA_TEXT, text)
          clipData = ClipData.newRawUri(null, uri)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        runOnUiThread { startActivity(Intent.createChooser(send, null)) }
        true
      } catch (e: Exception) {
        Log.w("Birdfeeder", "Could not share the card", e)
        false
      }
    }
  }

  // Dark mode changes without the activity starting over, so the bars and window follow by hand.
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    matchTheme()
  }

  // The activity itself stays on the phone's theme, so the web view keeps telling the page
  // what the phone uses. Only the bar icons and the window behind the page follow the choice.
  private fun matchTheme() {
    val night = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
    val dark = theme == "dark" || (theme == "system" && night == Configuration.UI_MODE_NIGHT_YES)
    val bars = if (dark) {
      SystemBarStyle.dark(Color.TRANSPARENT)
    } else {
      SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
    }
    enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = bars)
    window.setBackgroundDrawable(ColorDrawable(getColor(if (dark) R.color.paper_dark else R.color.paper_light)))
  }

  companion object {
    private const val PREFS = "appearance"
    private const val THEME = "theme"
  }
}
