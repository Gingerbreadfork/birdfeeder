package io.github.gingerbreadfork.birdfeeder

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import org.json.JSONObject

/**
 * Checks GitHub while Birdfeeder is closed, and notifies about what turned up. Android starts
 * it with the network the job asked for, which an app in the background otherwise goes without.
 */
class CheckJob : JobService() {
  override fun onStartJob(params: JobParameters): Boolean {
    // The app checks by itself while it is on screen.
    if (ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
      return false
    }
    thread(name = "Birdfeeder check") {
      try {
        check(applicationContext)
      } catch (e: Throwable) {
        Log.w(TAG, "Background check failed", e)
      }
      // A failed check waits for the next one rather than backing off.
      jobFinished(params, false)
    }
    return true
  }

  // The check in Rust cannot be stopped part way, and the next one comes round anyway.
  override fun onStopJob(params: JobParameters): Boolean = false

  companion object {
    private const val TAG = "Birdfeeder"
    private const val PERIODIC = 1
    private const val SOON = 2
    private const val CHANNEL = "activity"
    private const val PREFS = "notifications"

    @JvmStatic external fun checkOnce(dir: String): String

    private fun job(context: Context, id: Int) =
      JobInfo.Builder(id, ComponentName(context, CheckJob::class.java))
        .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)

    /** Asks Android to run a check about every 15 minutes while there is a connection. */
    fun schedule(context: Context) {
      val scheduler = context.getSystemService(JobScheduler::class.java)
      if (scheduler.getPendingJob(PERIODIC) != null) return
      scheduler.schedule(
        job(context, PERIODIC).setPeriodic(TimeUnit.MINUTES.toMillis(15)).setPersisted(true).build()
      )
    }

    /** Runs one check soon, with time to leave the app first so it runs as it would when closed. */
    fun runSoon(context: Context) {
      context.getSystemService(JobScheduler::class.java)
        .schedule(job(context, SOON).setMinimumLatency(TimeUnit.SECONDS.toMillis(10)).build())
    }

    private fun check(context: Context) {
      System.loadLibrary("birdfeeder_lib")
      // The same folder Tauri gives the app for its data.
      val answer = JSONObject(checkOnce(context.dataDir.absolutePath))
      if (!answer.isNull("error")) Log.w(TAG, "Background check failed: ${answer.getString("error")}")
      if (!answer.optBoolean("keep", false)) {
        context.getSystemService(JobScheduler::class.java).cancel(PERIODIC)
        return
      }
      val notices = answer.optJSONArray("notices") ?: return
      for (i in 0 until notices.length()) {
        val notice = notices.getJSONObject(i)
        post(context, notice.optString("title"), notice.optString("body"))
      }
    }

    private fun post(context: Context, title: String, body: String) {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
        PackageManager.PERMISSION_GRANTED
      ) {
        return
      }
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val channel = NotificationChannel(CHANNEL, "GitHub activity", NotificationManager.IMPORTANCE_DEFAULT)
        channel.description = "Stars, followers, issues, mentions and milestones"
        context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
      }
      val open = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
      )
      val notification = NotificationCompat.Builder(context, CHANNEL)
        .setSmallIcon(R.drawable.ic_stat_bird)
        .setColor(ContextCompat.getColor(context, R.color.seed))
        .setContentTitle(title)
        .setContentText(body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(body))
        .setContentIntent(open)
        .setAutoCancel(true)
        .build()
      // Each notification gets a number of its own, so a new one never replaces an old one.
      val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
      val id = prefs.getInt("next", 1)
      prefs.edit().putInt("next", if (id == Int.MAX_VALUE) 1 else id + 1).apply()
      NotificationManagerCompat.from(context).notify(id, notification)
    }
  }
}
