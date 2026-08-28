package com.handy.voice

import android.animation.ValueAnimator
import android.os.Bundle
import android.view.View
import androidx.activity.addCallback
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.handy.voice.databinding.ActivityIntroBinding

/**
 * Вводный экран: четыре шага о том, что приложение делает и что для этого
 * нужно разрешить. Показывается один раз при первом запуске — дальше только
 * по просьбе из «О проекте».
 */
class IntroActivity : AppCompatActivity() {

    private lateinit var ui: ActivityIntroBinding
    private var step = 0

    private data class Step(val icon: Int, val title: Int, val text: Int)

    private val steps = listOf(
        Step(R.drawable.ic_waveform, R.string.intro1_title, R.string.intro1_text),
        Step(R.drawable.ic_mic, R.string.intro2_title, R.string.intro2_text),
        Step(R.drawable.ic_text_lines, R.string.intro3_title, R.string.intro3_text),
        Step(R.drawable.ic_file, R.string.intro4_title, R.string.intro4_text),
    )

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = ActivityIntroBinding.inflate(layoutInflater)
        setContentView(ui.root)

        val density = resources.displayMetrics.density
        val edge = (32 * density).toInt()
        ViewCompat.setOnApplyWindowInsetsListener(ui.root) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val extra = ((view.width - (640 * density).toInt()) / 2).coerceAtLeast(0)
            view.setPadding(edge + extra, bars.top + edge, edge + extra, bars.bottom)
            insets
        }

        ui.next.setOnClickListener {
            if (step == steps.lastIndex) done() else render(step + 1)
        }
        ui.skip.setOnClickListener { done() }
        // Системный «назад» листает шаги обратно, а с первого закрывает экран.
        onBackPressedDispatcher.addCallback(this) {
            if (step > 0) render(step - 1) else done()
        }

        render(0)
    }

    private fun render(next: Int) {
        val forward = next > step
        step = next
        val current = steps[step]
        ui.icon.setImageResource(current.icon)
        ui.title.setText(current.title)
        ui.text.setText(current.text)
        ui.step.text = getString(R.string.intro_step, step + 1, steps.size)
        ui.next.setText(if (step == steps.lastIndex) R.string.intro_start else R.string.intro_next)
        ui.skip.visibility = if (step == steps.lastIndex) View.GONE else View.VISIBLE
        slide(forward)
    }

    /** Шаг въезжает с той стороны, куда пошёл пользователь — как вкладки. */
    private fun slide(forward: Boolean) {
        if (!ValueAnimator.areAnimatorsEnabled()) return
        val shift = 28 * resources.displayMetrics.density * if (forward) 1 else -1
        for (view in listOf(ui.icon, ui.title, ui.text)) {
            view.translationX = shift
            view.alpha = 0f
            view.animate()
                .translationX(0f)
                .alpha(1f)
                .setDuration(resources.getInteger(R.integer.anim_base).toLong())
                .start()
        }
    }

    private fun done() {
        AppPrefs.setIntroShown(this, true)
        finish()
    }
}
