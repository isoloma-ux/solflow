package com.handy.voice

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.MediaPlayer
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.os.PowerManager
import android.provider.Settings
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import android.widget.Toast
import android.animation.Animator
import android.animation.ValueAnimator
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.animation.DecelerateInterpolator
import android.widget.ImageView
import androidx.activity.addCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.drawerlayout.widget.DrawerLayout
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.snackbar.Snackbar
import androidx.core.content.ContextCompat
import androidx.core.view.GravityCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.TextViewCompat
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.handy.voice.databinding.ActivityMainBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.system.measureTimeMillis

class MainActivity : AppCompatActivity() {

    private lateinit var ui: ActivityMainBinding
    private var recorder = AudioRecorder(false)

    private lateinit var models: ModelAdapter
    private lateinit var history: HistoryAdapter
    private lateinit var meetings: MeetingAdapter
    private lateinit var segments: SegmentAdapter

    private var catalog: List<CatalogModel> = emptyList()
    private var languageFilter: String? = null
    private var modelQuery = ""
    private var onlyDownloaded = false
    private var stateJob: Job? = null
    private val quantRows = mutableMapOf<String, Pair<TextView, ProgressBar>>()
    private var quantRender: (() -> Unit)? = null
    private var permissionsExpanded = false
    private var page = Page.DICTATION
    private var openMeetingId: Long? = null
    private var micAsked = false
    private var selectionShown = false
    /** null — все встречи, [NO_PROJECT] — те, что вне проектов. */
    private var projectFilter: String? = null
    private var meetingQuery = ""
    private val selection = mutableSetOf<Long>()

    /** Реплики открытой встречи и места, где нашлось искомое слово. */
    private var detailSegments: List<MeetingSegment> = emptyList()
    private var findMatches: List<Int> = emptyList()
    private var findIndex = -1

    /** Что подставить в поиск и куда прокрутить сразу после открытия. */
    private var pendingFind: String? = null
    private var pendingIndex: Int? = null
    private var player: MediaPlayer? = null

    /** Что уже показано в таймлайне — чтобы не перечитывать JSON зря. */
    private var shownTranscript: Pair<Long, String>? = null

    /** Что уже отдано списку встреч — чтобы не пересобирать его зря. */
    private var shownMeetingList: String? = null

    private enum class Page { DICTATION, MEETINGS, HISTORY, MODELS }

    private companion object {
        const val NO_PROJECT = "__none__"
        const val ALL_PROJECTS = "__all__"
        const val NEW_PROJECT = "__new__"
        const val RENAME_PROJECT = "__rename__"
        const val DELETE_PROJECT = "__delete__"
        const val STATE_POLL_MS = 600L

        /** До скольких реплик переход к совпадению едет прокруткой. */
        const val NEAR_JUMP = 20
        const val WAVE_TICK_MS = 80L
    }

    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        renderPermissions()
        prepareEngine()
    }
    private val askNotifications =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { renderPermissions() }
    /**
     * Видео берём наравне со звуком: внутри mp4 или mkv лежит обычная
     * звуковая дорожка, а [AudioImport] и так выбирает из файла именно её.
     */
    private val pickAudio =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) MeetingService.import(this, uri)
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = ActivityMainBinding.inflate(layoutInflater)
        setContentView(ui.root)

        // Android 15+ рисует под системными панелями и игнорирует
        // fitsSystemWindows, поэтому отступы считаем сами. 32dp — токен pad.
        // На широких экранах (планшет, ландшафт) контент держится в колонке
        // ~640dp: строки во всю ширину планшета нечитаемы.
        val density = resources.displayMetrics.density
        val edge = (32 * density).toInt()
        ViewCompat.setOnApplyWindowInsetsListener(ui.content) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            // Колонка по центру нужна только там, где разделы внизу: с
            // рельсом слева экран уже разделён, и лишние отступы сдвигали бы
            // содержимое вправо.
            val extra = if (resources.getBoolean(R.bool.wide_layout)) 0
            else ((view.width - (640 * density).toInt()) / 2).coerceAtLeast(0)
            view.setPadding(edge + extra, bars.top + edge, edge + extra, bars.bottom)
            // Шторка лежит поверх контента и системные отступы не наследует:
            // без этого её заголовок уезжал под часы в строке состояния.
            ui.drawerPanel.setPadding(
                ui.drawerPanel.paddingStart, bars.top,
                ui.drawerPanel.paddingEnd, bars.bottom,
            )
            insets
        }
        // При первом проходе insets ширина ещё нулевая — пересчитываем после
        // изменения размеров.
        ui.content.addOnLayoutChangeListener { v, l, _, r, _, ol, _, or_, _ ->
            if (r - l != or_ - ol) v.requestApplyInsets()
            sizeTabs()
        }

        ModelStore.migrateLegacyLayout(this)
        catalog = Catalog.models(this)

        setupDictation()
        setupMeetings()
        setupModels()
        setupHistory()

        for (menu in listOf(
            ui.pageDictation.menuDictation, ui.pageMeetings.menuMeetings,
            ui.pageHistory.menuHistory, ui.pageModels.menuModels,
        )) {
            menu.setOnClickListener { ui.drawer.openDrawer(GravityCompat.START) }
        }
        ui.drawer.addDrawerListener(object : DrawerLayout.SimpleDrawerListener() {
            // Проекты могли поменяться на вкладке встреч — шторка каждый раз
            // собирается заново, список короткий.
            override fun onDrawerOpened(drawerView: View) = renderDrawer()
        })
        renderDrawer()

        ui.navDictation.setOnClickListener { show(Page.DICTATION) }
        ui.navMeetings.setOnClickListener { show(Page.MEETINGS) }
        ui.navHistory.setOnClickListener { show(Page.HISTORY) }
        ui.navModels.setOnClickListener { show(Page.MODELS) }
        show(Page.DICTATION)

        // Свайп по контенту листает вкладки, как их тап внизу. Внутри
        // открытой встречи свайп вправо ведёт назад к списку — это тот же
        // «шаг назад», что и кнопка.
        ui.pages.onSwipe = { forward ->
            if (page == Page.MEETINGS && openMeetingId != null) {
                if (!forward) {
                    openMeetingId = null
                    renderMeetings()
                }
            } else {
                val target = page.ordinal + if (forward) 1 else -1
                Page.entries.getOrNull(target)?.let { show(it) }
            }
        }

        ui.content.post { sizeTabs() }

        // Системный «назад» (жест или стрелка) идёт по тем же ступеням, что
        // и кнопки в приложении: деталь встречи → список → вкладка диктовки,
        // и только с неё сворачивает приложение.
        onBackPressedDispatcher.addCallback(this) {
            when {
                ui.drawer.isDrawerOpen(GravityCompat.START) ->
                    ui.drawer.closeDrawer(GravityCompat.START)
                page == Page.MEETINGS && selection.isNotEmpty() -> clearSelection()
                page == Page.MEETINGS && openMeetingId != null -> {
                    openMeetingId = null
                    renderMeetings()
                }
                page != Page.DICTATION -> show(Page.DICTATION)
                else -> moveTaskToBack(true)
            }
        }

        handleShared(intent)

        // Вводный экран идёт первым: просить микрофон раньше, чем человек
        // понял, зачем приложение, — верный способ получить отказ.
        if (!AppPrefs.introShown(this)) {
            // Свежая установка: вводного экрана достаточно, «Что нового»
            // человеку без «старого» показывать не с чего.
            AppPrefs.setLastSeenVersion(this, BuildConfig.VERSION_CODE)
            startActivity(Intent(this, IntroActivity::class.java))
        } else {
            val lastSeen = AppPrefs.lastSeenVersion(this)
            if (lastSeen < BuildConfig.VERSION_CODE) {
                AppPrefs.setLastSeenVersion(this, BuildConfig.VERSION_CODE)
                showWhatsNew(lastSeen)
            }
            if (!hasMic()) {
                micAsked = true
                askMic.launch(Manifest.permission.RECORD_AUDIO)
            }
        }
    }

    /**
     * Один раз после обновления: что изменилось. Перепрыгнувшим через
     * версию показываются и пропущенные разделы — историю ведет список
     * ниже, новые версии дописываются в его начало.
     */
    private fun showWhatsNew(lastSeenCode: Int) {
        val history = listOf(
            Triple(27, "0.5.0", R.string.whatsnew_body),
            Triple(26, "0.4.0", R.string.whatsnew_body_040),
        )
        val message = buildString {
            for ((code, name, body) in history) {
                if (code <= lastSeenCode && lastSeenCode != 0) continue
                if (isNotEmpty()) {
                    append("\n\n")
                    append(getString(R.string.whatsnew_earlier, name))
                    append("\n\n")
                }
                append(getString(body))
            }
        }
        if (message.isBlank()) return
        MaterialAlertDialogBuilder(this)
            .setTitle(getString(R.string.whatsnew_title, BuildConfig.VERSION_NAME))
            .setMessage(message)
            .setPositiveButton(R.string.whatsnew_ok, null)
            .show()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleShared(intent)
    }

    /**
     * Файл, отданный приложению через «Поделиться» или «Открыть с помощью».
     * На телефоне это главный способ принести запись: видео и аудио лежат в
     * мессенджерах и облаках, а не в папке, которую удобно выбирать руками.
     */
    private fun handleShared(intent: Intent?) {
        // Ссылкой делятся текстом — это отдельный путь, файла тут нет.
        val text = intent?.takeIf { it.action == Intent.ACTION_SEND }
            ?.getStringExtra(Intent.EXTRA_TEXT)?.trim()
        if (!text.isNullOrEmpty()) {
            intent.action = null
            startLink(text)
            return
        }

        val uri = when (intent?.action) {
            Intent.ACTION_SEND -> intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
            Intent.ACTION_VIEW -> intent.data
            else -> null
        } ?: return

        // Разрешение на чтение живёт вместе с этим намерением — забираем
        // ссылку сразу, чтобы повторный вход не начал импорт заново.
        intent?.action = null
        MeetingService.import(this, uri)
        show(Page.MEETINGS)
    }

    /**
     * Крестик очистки в поле поиска: появляется, когда есть что стирать.
     * Рисуется compound-иконкой справа — отдельная кнопка раздувала бы
     * разметку ради одного действия.
     */
    @Suppress("ClickableViewAccessibility")
    private fun clearableSearch(edit: EditText) {
        fun update() {
            val icon = if (edit.text.isNullOrEmpty()) null else getDrawable(R.drawable.ic_clear)
            edit.setCompoundDrawablesRelativeWithIntrinsicBounds(null, null, icon, null)
        }
        update()
        edit.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable?) = update()
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        edit.setOnTouchListener { _, event ->
            val icon = edit.compoundDrawablesRelative[2]
            if (event.action == MotionEvent.ACTION_UP && icon != null &&
                event.x >= edit.width - edit.paddingEnd - icon.bounds.width()
            ) {
                edit.setText("")
                true
            } else {
                false
            }
        }
    }

    // --- движение ---------------------------------------------------------

    /** Пользователь мог выключить анимации в системе — уважаем это везде. */
    private fun motionOn() = ValueAnimator.areAnimatorsEnabled()

    private val animBase get() = resources.getInteger(R.integer.anim_base).toLong()

    /**
     * Мягкое появление страницы. [direction] задаёт сторону въезда: +1 —
     * справа (пошли вперёд), -1 — слева (назад), 0 — подъём снизу.
     */
    private fun enterPage(view: View, direction: Int = 0) {
        view.visibility = View.VISIBLE
        if (!motionOn()) return
        val density = resources.displayMetrics.density
        view.alpha = 0f
        if (direction == 0) {
            view.translationY = 10 * density
        } else {
            view.translationX = direction * 28 * density
        }
        view.animate().alpha(1f).translationY(0f).translationX(0f)
            .setDuration(animBase)
            .setInterpolator(DecelerateInterpolator())
            .start()
    }

    /** Каскад строк списка при показе вкладки. */
    private fun stagger(list: RecyclerView) {
        if (motionOn()) list.scheduleLayoutAnimation() else list.layoutAnimation = null
    }

    /**
     * Пульс кольца за кнопкой записи: расходится и тает, пока идёт запись.
     * Движение здесь не украшение — это индикатор «микрофон живой».
     */
    private fun startPulse(view: View): Animator = ValueAnimator.ofFloat(0f, 1f).apply {
        duration = 1400
        repeatCount = ValueAnimator.INFINITE
        addUpdateListener { va ->
            val t = va.animatedFraction
            view.scaleX = 1f + 0.5f * t
            view.scaleY = 1f + 0.5f * t
            view.alpha = (1f - t) * 0.8f
        }
        start()
    }

    private fun stopPulse(animator: Animator?, view: View) {
        animator?.cancel()
        view.alpha = 0f
        view.scaleX = 1f
        view.scaleY = 1f
    }

    private fun haptic(view: View) {
        view.performHapticFeedback(
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                HapticFeedbackConstants.CONFIRM
            } else {
                HapticFeedbackConstants.KEYBOARD_TAP
            }
        )
    }

    override fun onResume() {
        super.onResume()
        if (!micAsked && AppPrefs.introShown(this) && !hasMic()) {
            micAsked = true
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        }
        prepareEngine()
        refreshModels()
        refreshHistory()
        watchState()
        // Загрузка живёт в сервисе и идёт даже со свёрнутым приложением —
        // экран просто слушает её и перерисовывается.
        DownloadService.onChange = {
            runOnUiThread {
                renderDownloadSummary()
                refreshModels()
                quantRender?.invoke()
            }
        }
        renderDownloadSummary()
        MeetingService.onChange = {
            runOnUiThread { if (page == Page.MEETINGS) renderMeetings() }
        }
        // Диктовка могла продолжаться, пока экран был свёрнут, — вернуть ей
        // пульс и волну.
        if (recorder.isRecording) {
            if (motionOn() && dictationPulse == null) {
                dictationPulse = startPulse(ui.pageDictation.dictationPulse)
            }
            if (dictationWaveJob == null) {
                dictationWaveJob = lifecycleScope.launch {
                    while (true) {
                        ui.pageDictation.dictationWave.push(recorder.level)
                        delay(WAVE_TICK_MS)
                    }
                }
            }
        }
    }

    override fun onPause() {
        stopPlayback()
        stateJob?.cancel()
        DownloadService.onChange = null
        MeetingService.onChange = null
        // Волны и пульсы не должны тикать за свёрнутым экраном.
        dictationWaveJob?.cancel()
        dictationWaveJob = null
        dictationPulse?.cancel()
        dictationPulse = null
        meetingWaveJob?.cancel()
        meetingWaveJob = null
        meetingPulse?.cancel()
        meetingPulse = null
        super.onPause()
    }

    /**
     * Пока экран открыт, сверяем показания с реальностью.
     *
     * Разрешения и сервис живут своей жизнью: спецвозможности гаснут при
     * обновлении приложения, сервис может подняться уже после того, как экран
     * отрисовался. Читать состояние один раз при открытии нельзя — экран
     * показывал «Включить» при работающей кнопке и «выдано» у выключенного
     * разрешения.
     */
    private fun watchState() {
        stateJob?.cancel()
        stateJob = lifecycleScope.launch {
            var previous: List<Boolean>? = null
            while (true) {
                val now = listOf(
                    hasMic(),
                    hasOverlay(),
                    HandyAccessibilityService.isGranted(this@MainActivity),
                    hasNotifications(),
                    ignoresBatteryOptimizations(),
                    DictationService.running,
                )
                if (now != previous) {
                    previous = now
                    renderPermissions()
                }
                // Таймер записи и проценты расшифровки живут в сервисе —
                // вкладка встреч просто перерисовывается, пока открыта.
                if (page == Page.MEETINGS) renderMeetings()
                delay(STATE_POLL_MS)
            }
        }
    }

    // --- навигация -------------------------------------------------------

    private fun show(next: Page) {
        val previous = page
        page = next
        val pages = listOf(
            Page.DICTATION to ui.pageDictation.root,
            Page.MEETINGS to ui.pageMeetings.root,
            Page.HISTORY to ui.pageHistory.root,
            Page.MODELS to ui.pageModels.root,
        )
        // Страница въезжает с той стороны, куда пользователь пошёл по ряду
        // вкладок, — и по тапу, и по свайпу.
        val direction = (next.ordinal - previous.ordinal).coerceIn(-1, 1)
        for ((value, root) in pages) {
            if (value == next) {
                if (root.visibility != View.VISIBLE && previous != next) {
                    enterPage(root, direction)
                } else {
                    root.visibility = View.VISIBLE
                }
            } else {
                root.animate().cancel()
                root.visibility = View.GONE
                root.alpha = 1f
                root.translationY = 0f
                root.translationX = 0f
            }
        }

        for ((tab, value) in listOf(
            ui.navDictation to Page.DICTATION,
            ui.navMeetings to Page.MEETINGS,
            ui.navHistory to Page.HISTORY,
            ui.navModels to Page.MODELS,
        )) {
            val on = value == next
            tab.setTextColor(getColor(if (on) R.color.ink else R.color.fog))
            tab.typeface = resources.getFont(if (on) R.font.inter_medium else R.font.inter_regular)
            // Зелёное подчёркивание активного пункта — тот же приём, что в
            // фильтрах каталога и в меню на сайте.
            tab.setBackgroundResource(if (on) R.drawable.bg_tab_active else 0)
        }
        if (next == Page.HISTORY) {
            refreshHistory()
            stagger(ui.pageHistory.historyList)
        }
        if (next == Page.MEETINGS) {
            renderMeetings()
            if (openMeetingId == null) stagger(ui.pageMeetings.meetingList)
        }
        if (next == Page.MODELS) stagger(ui.pageModels.modelList)
    }

    private fun visibility(shown: Boolean) = if (shown) View.VISIBLE else View.GONE

    /**
     * На крупном системном шрифте названия вкладок не влезают в ячейки —
     * пусть слегка ужимаются, обрезать навигацию нельзя. Предел ширины
     * известен только после раскладки, поэтому зовём и после неё.
     *
     * Проверка ширины обязательна: при смене темы активити пересоздаётся,
     * пока экран закрыт настройками, раскладки ещё нет — и `maxWidth = 0`
     * схлопывал весь ряд вкладок в невидимую полоску.
     */
    private fun sizeTabs() {
        for (tab in listOf(ui.navDictation, ui.navMeetings, ui.navHistory, ui.navModels)) {
            val cell = tab.parent as? View ?: continue
            if (cell.width <= 0) return
            // Слушатель раскладки зовёт нас на каждый проход: без этой
            // проверки maxWidth и авторазмер запрашивали бы раскладку заново
            // прямо во время неё — бесконечный круг и мусор в логах.
            if (tab.maxWidth == cell.width) continue
            tab.maxWidth = cell.width
            TextViewCompat.setAutoSizeTextTypeUniformWithConfiguration(
                tab, 11, 16, 1, android.util.TypedValue.COMPLEX_UNIT_SP
            )
        }
    }

    // --- диктовка --------------------------------------------------------

    private fun setupDictation() {
        ui.pageDictation.record.setOnClickListener { toggleRecording() }
        ui.pageDictation.copy.setOnClickListener {
            copyToClipboard(ui.pageDictation.result.text?.toString().orEmpty())
        }
        ui.pageDictation.bubbleToggle.setOnClickListener { toggleBubble() }
        ui.pageDictation.modelName.setOnClickListener { chooseModel() }
    }

    // --- боковая шторка ---------------------------------------------------

    /**
     * Второй уровень навигации: проекты встреч и разделы приложения.
     *
     * Разделы остались вкладками внизу — на телефоне это норма Material, а
     * бургер вместо них добавил бы тап к каждому переходу и спрятал бы сами
     * разделы. В шторку уходит то, что вкладкой быть не должно: список
     * проектов, настройки и «О проекте».
     */
    /**
     * Подвал бокового меню: активная модель и версия — как в боковой панели
     * на компьютере. Обе строки нажимаются: модель меняется на месте,
     * версия проверяет обновление и предлагает его поставить.
     */
    private fun renderDrawerFoot() {
        val active = ModelStore.activeFilename(this)
        val name = active?.let { Catalog.findByFilename(this, it)?.first?.name }
        ui.drawerModel.text = name ?: getString(R.string.model_pick_empty)
        ui.drawerModel.setOnClickListener {
            ui.drawer.closeDrawers()
            chooseModel()
        }

        ui.drawerVersion.text = getString(R.string.about_version, BuildConfig.VERSION_NAME)
        ui.drawerVersion.setOnClickListener { checkUpdateFromDrawer() }
    }

    /**
     * Проверка обновления из меню. Нашлось — предлагаем поставить прямо
     * отсюда: заходить ради этого в «О проекте» незачем.
     */
    private fun checkUpdateFromDrawer() {
        val version = ui.drawerVersion
        version.setText(R.string.update_checking)
        lifecycleScope.launch {
            val release = withContext(Dispatchers.IO) { AppUpdate.latest() }
            if (release == null || !newerVersion(release.version, BuildConfig.VERSION_NAME)) {
                version.setText(
                    if (release == null) R.string.update_failed else R.string.update_current
                )
                // Через несколько секунд возвращаем обычную подпись: подвал
                // не место для отчётов.
                version.postDelayed({
                    version.text = getString(R.string.about_version, BuildConfig.VERSION_NAME)
                }, 4_000)
                return@launch
            }
            version.text = getString(R.string.update_available, release.version)
            version.setOnClickListener {
                startActivity(Intent(this@MainActivity, AboutActivity::class.java))
            }
        }
    }

    /** Сравнение версий по числам: «0.3.1» новее «0.3.0». */
    private fun newerVersion(latest: String, current: String): Boolean {
        fun parts(v: String) = v.trimStart('v', 'V').split('.')
            .map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        val a = parts(latest)
        val b = parts(current)
        for (i in 0 until maxOf(a.size, b.size)) {
            val l = a.getOrElse(i) { 0 }
            val c = b.getOrElse(i) { 0 }
            if (l != c) return l > c
        }
        return false
    }

    /**
     * Шторка в три блока, разделённых волосками: «Все встречи» наверху сама
     * по себе, проекты со своим заголовком и приглушённым «+ Новый проект»,
     * внизу — разделы приложения. Раньше всё сливалось в один столбик, и
     * «Новый проект» читался как ещё один проект.
     */
    private fun renderDrawer() {
        renderDrawerFoot()
        val box = ui.drawerList
        box.removeAllViews()

        drawerRow(getString(R.string.project_all), projectFilter == null) { pickProject(null) }

        drawerDivider()
        drawerGroup(R.string.drawer_projects)
        drawerRow(getString(R.string.project_none), projectFilter == NO_PROJECT) {
            pickProject(NO_PROJECT)
        }
        for (project in MeetingStore.projects(this)) {
            drawerRow(project.name, projectFilter == project.id) { pickProject(project.id) }
        }
        drawerRow("+  " + getString(R.string.project_new_title), false, muted = true) {
            ui.drawer.closeDrawer(GravityCompat.START)
            askProjectName(null) { name ->
                pickProject(MeetingStore.createProject(this, name).id)
            }
        }

        drawerDivider()
        drawerGroup(R.string.drawer_app)
        drawerRow(getString(R.string.tab_settings), false) {
            ui.drawer.closeDrawer(GravityCompat.START)
            startActivity(Intent(this, SettingsActivity::class.java))
        }
        drawerRow(getString(R.string.about_title), false) {
            ui.drawer.closeDrawer(GravityCompat.START)
            startActivity(Intent(this, AboutActivity::class.java))
        }
    }

    private fun drawerGroup(title: Int) {
        val view = layoutInflater.inflate(R.layout.item_setting_group, ui.drawerList, false)
        (view as TextView).setText(title)
        // После волоска заголовку хватает небольшого отступа: свои 28dp он
        // носит для настроек, где волосков нет.
        (view.layoutParams as LinearLayout.LayoutParams).topMargin =
            (10 * resources.displayMetrics.density).toInt()
        ui.drawerList.addView(view)
    }

    /** Волосок между блоками шторки. */
    private fun drawerDivider() {
        val density = resources.displayMetrics.density
        val line = View(this)
        line.setBackgroundColor(getColor(R.color.hairline))
        val lp = LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            (1 * density).toInt().coerceAtLeast(1),
        )
        lp.topMargin = (10 * density).toInt()
        ui.drawerList.addView(line, lp)
    }

    private fun drawerRow(
        title: String,
        active: Boolean,
        muted: Boolean = false,
        onTap: () -> Unit,
    ) {
        val row = layoutInflater
            .inflate(R.layout.item_drawer_row, ui.drawerList, false) as TextView
        row.text = title
        if (active) {
            row.setTextColor(getColor(R.color.accent))
            row.typeface = resources.getFont(R.font.inter_medium)
        }
        // Приглушённая строка — действие, а не пункт списка.
        if (muted) row.setTextColor(getColor(R.color.fog))
        row.setOnClickListener { onTap() }
        ui.drawerList.addView(row)
    }

    /** Выбор проекта в шторке ведёт на вкладку встреч с этим фильтром. */
    private fun pickProject(id: String?) {
        projectFilter = id
        ui.drawer.closeDrawer(GravityCompat.START)
        show(Page.MEETINGS)
        renderMeetings()
        renderDrawer()
    }

    private fun prepareEngine() {
        val active = ModelStore.activeFile(this)
        if (active == null) {
            ui.pageDictation.modelName.setText(R.string.model_pick_empty)
            ui.pageDictation.status.setText(R.string.no_model)
            ui.pageDictation.record.isEnabled = false
            return
        }
        ui.pageDictation.modelName.text = buildString {
            append(Catalog.findByFilename(this@MainActivity, active.name)?.first?.name ?: active.name)
            append(" ▾")
        }

        if (Engine.currentModel == active.name) {
            ui.pageDictation.status.setText(R.string.ready)
            ui.pageDictation.record.isEnabled = hasMic()
            return
        }

        ui.pageDictation.record.isEnabled = false
        ui.pageDictation.status.setText(R.string.loading_model)
        lifecycleScope.launch {
            val ok = Engine.ensureLoaded(this@MainActivity)
            ui.pageDictation.status.setText(if (ok) R.string.ready else R.string.model_failed)
            ui.pageDictation.record.isEnabled = ok && hasMic()
        }
    }

    /**
     * Быстрая смена модели прямо с экрана диктовки: скачанных обычно две-три,
     * и уходить ради переключения в каталог — лишний путь.
     */
    private fun chooseModel() {
        val rows = ModelStore.downloadedFilenames(this).map { filename ->
            val found = Catalog.findByFilename(this, filename)
            Triple(filename, found?.first?.name ?: filename, found?.second?.quant)
        }.sortedBy { it.second }

        if (rows.isEmpty()) {
            Snackbar.make(ui.root, R.string.model_pick_empty_hint, Snackbar.LENGTH_LONG)
                .setAction(R.string.tab_models) { show(Page.MODELS) }
                .show()
            return
        }

        // Одинаковые названия различаем квантованием: у одной модели можно
        // держать несколько версий разом.
        val twice = rows.groupBy { it.second }.filterValues { it.size > 1 }.keys
        val options = rows.map { (filename, name, quant) ->
            filename to if (name in twice && quant != null) "$name · $quant" else name
        }

        optionSheet(
            getString(R.string.model_pick),
            options,
            ModelStore.activeFilename(this),
            hint = if (options.size > 1) null else getString(R.string.model_pick_hint),
        ) { filename ->
            if (filename == ModelStore.activeFilename(this)) return@optionSheet
            ModelStore.setActive(this, filename)
            // Движок подхватит новую модель сам: prepareEngine видит, что
            // загружена другая, и переставляет её.
            prepareEngine()
            refreshModels()
        }
    }

    private var dictationPulse: Animator? = null
    private var dictationWaveJob: Job? = null

    private fun toggleRecording() {
        if (recorder.isRecording) {
            stopAndTranscribe()
            return
        }
        // Режим микрофона и выбранное устройство берём заново: их могли
        // поменять в настройках, пока экран был открыт.
        recorder = AudioRecorder(AppPrefs.roomMode(this), MicDevices.preferred(this))
        if (recorder.start()) {
            AudioSession.playStart(this)
            AudioSession.mute(this)
            haptic(ui.pageDictation.record)
            ui.pageDictation.record.setIconResource(R.drawable.ic_stop)
            ui.pageDictation.status.setText(R.string.recording)
            ui.pageDictation.dictationWave.visibility = View.VISIBLE
            if (motionOn()) dictationPulse = startPulse(ui.pageDictation.dictationPulse)
            dictationWaveJob = lifecycleScope.launch {
                while (true) {
                    ui.pageDictation.dictationWave.push(recorder.level)
                    delay(WAVE_TICK_MS)
                }
            }
        } else {
            ui.pageDictation.status.setText(R.string.mic_failed)
        }
    }

    private fun stopDictationMotion() {
        stopPulse(dictationPulse, ui.pageDictation.dictationPulse)
        dictationPulse = null
        dictationWaveJob?.cancel()
        dictationWaveJob = null
        ui.pageDictation.dictationWave.visibility = View.GONE
        ui.pageDictation.dictationWave.reset()
    }

    private fun stopAndTranscribe() {
        val pcm = recorder.stop()
        AudioSession.unmute(this)
        haptic(ui.pageDictation.record)
        stopDictationMotion()
        ui.pageDictation.record.setIconResource(R.drawable.ic_mic)
        ui.pageDictation.record.isEnabled = false

        val seconds = pcm.size.toFloat() / AudioRecorder.SAMPLE_RATE
        if (seconds < 0.3f) {
            ui.pageDictation.status.setText(R.string.too_short)
            ui.pageDictation.record.isEnabled = true
            return
        }

        ui.pageDictation.status.setText(R.string.transcribing)
        lifecycleScope.launch {
            var text = ""
            // Модель могла выгрузиться по таймеру, пока экран был открыт, —
            // без проверки распознавание молча вернуло бы пустоту.
            val ready = Engine.ensureLoaded(this@MainActivity)
            val ms = measureTimeMillis {
                text = if (!ready) "" else withContext(Dispatchers.Default) {
                    Engine.transcribe(
                        pcm,
                        AudioRecorder.SAMPLE_RATE,
                        AppPrefs.removeFillers(this@MainActivity),
                    )
                }
            }
            Engine.scheduleUnload(this@MainActivity)
            ui.pageDictation.result.text = text.ifBlank { getString(R.string.nothing_heard) }
            ui.pageDictation.copy.visibility = visibility(text.isNotBlank())
            if (text.isNotBlank()) TranscriptStore.add(this@MainActivity, text, seconds, pcm)

            val rt = if (ms > 0) seconds / (ms / 1000f) else 0f
            ui.pageDictation.status.text = getString(R.string.stats, seconds, ms / 1000f, rt)
            ui.pageDictation.record.isEnabled = true
        }
    }

    // --- встречи ----------------------------------------------------------

    private fun setupMeetings() {
        meetings = MeetingAdapter(
            onTap = { meeting ->
                // Пока идёт выбор нескольких, тап отмечает, а не открывает:
                // иначе из режима выбора невозможно выйти пальцем.
                if (selection.isNotEmpty()) toggleSelection(meeting) else {
                    // Из результатов поиска открываем сразу на первом
                    // совпадении: раньше человек попадал в начало и искал
                    // глазами то, что приложение уже нашло.
                    openMeeting(meeting.id, meetingQuery.trim())
                }
            },
            onLongTap = { toggleSelection(it) },
            onQuoteTap = { meeting, quote ->
                openMeeting(meeting.id, meetingQuery.trim(), quote.index)
            },
            onCancelWork = { meeting ->
                MeetingService.cancelWork(this, meeting.id)
                Toast.makeText(this, R.string.transcribe_cancelled, Toast.LENGTH_SHORT).show()
            },
        )
        segments = SegmentAdapter { renameSpeaker(it) }
        val m = ui.pageMeetings
        m.meetingList.layoutManager = LinearLayoutManager(this)
        m.meetingList.adapter = meetings
        m.meetingTimeline.layoutManager = LinearLayoutManager(this)
        m.meetingTimeline.adapter = segments

        m.meetingRecord.setOnClickListener { toggleMeetingRecording() }
        m.meetingPause.setOnClickListener {
            haptic(m.meetingPause)
            MeetingService.togglePause(this)
            renderMeetings()
        }
        // Тумблер живёт на время записи: комната по умолчанию, глушение
        // включают руками и только когда фон реально мешает.
        m.meetingSuppressSwitch.setOnCheckedChangeListener { _, on ->
            MeetingService.suppressNoise = on
        }
        m.meetingImport.setOnClickListener {
            pickAudio.launch(arrayOf("audio/*", "video/*"))
        }
        m.meetingLink.setOnClickListener { askLink() }
        m.meetingBack.setOnClickListener {
            openMeetingId = null
            renderMeetings()
        }
        m.meetingTitle.setOnClickListener { renameMeeting() }
        m.meetingTranscribe.setOnClickListener {
            openMeetingId?.let { MeetingService.transcribe(this, it) }
            renderMeetings()
        }
        m.meetingCancelWork.setOnClickListener {
            openMeetingId?.let { MeetingService.cancelWork(this, it) }
            Toast.makeText(this, R.string.transcribe_cancelled, Toast.LENGTH_SHORT).show()
            renderMeetings()
        }
        m.meetingCopy.setOnClickListener { copyMeeting() }
        m.meetingShareText.setOnClickListener { shareMeetingText() }
        m.meetingSaveAudio.setOnClickListener { saveMeetingAudio() }
        m.meetingDelete.setOnClickListener { deleteMeeting() }
        m.meetingDiarize.setOnClickListener { chooseDiarize() }
        m.exportTxt.setOnClickListener { exportMeeting(ExportFormat.TXT) }
        m.exportMd.setOnClickListener { exportMeeting(ExportFormat.MD) }
        m.exportPdf.setOnClickListener { exportMeeting(ExportFormat.PDF) }
        m.exportDoc.setOnClickListener { exportMeeting(ExportFormat.DOCX) }

        clearableSearch(m.meetingSearch)
        m.meetingSearch.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable?) {
                meetingQuery = s?.toString().orEmpty()
                renderMeetings()
            }

            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        clearableSearch(m.meetingFind)
        m.meetingFind.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable?) = applyFind(jump = true)
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        // Кнопка «найти» на клавиатуре и стрелка ведут к следующему месту.
        m.meetingFind.setOnEditorActionListener { _, _, _ ->
            nextMatch()
            true
        }
        m.meetingFindNext.setOnClickListener { nextMatch() }

        m.filterProject.setOnClickListener { chooseProject() }
        m.selectionCancel.setOnClickListener { clearSelection() }
        m.selectionDelete.setOnClickListener { deleteSelected() }
        m.selectionTranscribe.setOnClickListener { transcribeSelected() }
        m.selectionProject.setOnClickListener { moveSelected() }
        m.selectionExport.setOnClickListener { exportSelected() }
    }

    /**
     * Открывает встречу, при необходимости подставив поиск и прокрутив к
     * нужной реплике. Прокрутка откладывается: таймлайн заполняется уже в
     * [renderMeetings], до этого прокручивать нечего.
     */
    private fun openMeeting(id: Long, query: String = "", index: Int? = null) {
        openMeetingId = id
        pendingFind = query
        pendingIndex = index
        renderMeetings()
    }

    /**
     * Пересчитывает совпадения в открытой расшифровке и подсвечивает их.
     * [jump] — сразу перейти к первому: человек ищет, чтобы попасть в место,
     * а не чтобы полюбоваться подсветкой.
     */
    private fun applyFind(jump: Boolean) {
        val m = ui.pageMeetings
        val query = m.meetingFind.text?.toString()?.trim().orEmpty()
        findMatches = MeetingStore.matches(detailSegments, query)
        findIndex = -1
        segments.setQuery(query)
        m.meetingFindNext.visibility = visibility(findMatches.isNotEmpty())
        m.meetingFindCount.text = when {
            query.isBlank() -> ""
            findMatches.isEmpty() -> getString(R.string.search_nothing)
            else -> getString(R.string.find_count, findMatches.size)
        }
        if (jump && findMatches.isNotEmpty()) nextMatch()
    }

    /** Следующее совпадение по кругу. */
    private fun nextMatch() {
        if (findMatches.isEmpty()) return
        findIndex = (findIndex + 1) % findMatches.size
        jumpToSegment(findMatches[findIndex])
        ui.pageMeetings.meetingFindCount.text =
            getString(R.string.find_position, findIndex + 1, findMatches.size)
    }

    /** Ведёт таймлайн к реплике и отмечает её как текущую. */
    private fun jumpToSegment(index: Int) {
        val list = ui.pageMeetings.meetingTimeline
        val manager = list.layoutManager as? LinearLayoutManager
        // Соседнее совпадение доезжает прокруткой — так видно, куда именно
        // тебя перенесло. Через полсотни реплик прокрутка превратилась бы в
        // долгую тряску, поэтому туда переносим сразу.
        val near = manager != null &&
            kotlin.math.abs(index - manager.findFirstVisibleItemPosition()) <= NEAR_JUMP
        if (near && motionOn()) {
            list.smoothScrollToPosition(index)
        } else {
            manager?.scrollToPositionWithOffset(index, (24 * resources.displayMetrics.density).toInt())
        }
        segments.setQuery(
            ui.pageMeetings.meetingFind.text?.toString()?.trim().orEmpty(),
            index,
        )
    }

    // --- проекты и выбор нескольких ---------------------------------------

    private fun toggleSelection(meeting: Meeting) {
        if (!selection.add(meeting.id)) selection.remove(meeting.id)
        haptic(ui.pageMeetings.meetingList)
        renderMeetings()
    }

    private fun clearSelection() {
        selection.clear()
        renderMeetings()
    }

    /**
     * Папка-фильтр над списком. Переименование и удаление проекта живут в
     * том же листе: отдельная кнопка ради двух редких действий не нужна.
     */
    private fun chooseProject() {
        val projects = MeetingStore.projects(this)
        val options = buildList {
            add(ALL_PROJECTS to getString(R.string.project_all))
            add(NO_PROJECT to getString(R.string.project_none))
            for (p in projects) add(p.id to p.name)
            add(NEW_PROJECT to getString(R.string.project_new))
            projectFilter?.takeIf { it != NO_PROJECT }?.let {
                add(RENAME_PROJECT to getString(R.string.project_rename))
                add(DELETE_PROJECT to getString(R.string.project_delete))
            }
        }
        optionSheet(
            getString(R.string.project_pick),
            options,
            projectFilter ?: ALL_PROJECTS,
        ) { value ->
            when (value) {
                ALL_PROJECTS -> projectFilter = null
                NEW_PROJECT -> askProjectName(null) { name ->
                    projectFilter = MeetingStore.createProject(this, name).id
                    renderMeetings()
                }
                RENAME_PROJECT -> projectFilter?.let { id ->
                    askProjectName(MeetingStore.projectName(this, id)) { name ->
                        MeetingStore.renameProject(this, id, name)
                        renderMeetings()
                    }
                }
                DELETE_PROJECT -> projectFilter?.let { id -> confirmProjectDelete(id) }
                else -> projectFilter = value
            }
            renderMeetings()
        }
    }

    /** Куда положить отмеченные встречи. */
    private fun moveSelected() {
        val ids = selection.toList()
        if (ids.isEmpty()) return
        val options = buildList {
            add(NO_PROJECT to getString(R.string.project_none))
            for (p in MeetingStore.projects(this@MainActivity)) add(p.id to p.name)
            add(NEW_PROJECT to getString(R.string.project_new))
        }
        optionSheet(getString(R.string.selection_project), options, null) { value ->
            fun move(project: String?) {
                for (id in ids) MeetingStore.setProject(this, id, project)
                clearSelection()
            }
            when (value) {
                NO_PROJECT -> move(null)
                NEW_PROJECT -> askProjectName(null) { name ->
                    move(MeetingStore.createProject(this, name).id)
                }
                else -> move(value)
            }
        }
    }

    /**
     * Имя проекта спрашиваем диалогом с полем — так же, как имя говорящего.
     * Пустое имя не заводим: папка без названия неотличима от «без проекта».
     */
    private fun askProjectName(current: String?, onReady: (String) -> Unit) {
        val input = EditText(this).apply {
            setText(current.orEmpty())
            hint = getString(R.string.project_name_hint)
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(if (current == null) R.string.project_new_title else R.string.project_rename)
            .setView(input)
            .setPositiveButton(R.string.save) { _, _ ->
                val name = input.text?.toString()?.trim().orEmpty()
                if (name.isNotEmpty()) onReady(name)
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    private fun confirmProjectDelete(id: String) {
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.project_delete)
            .setMessage(R.string.project_delete_message)
            .setPositiveButton(R.string.meeting_delete) { _, _ ->
                MeetingStore.deleteProject(this, id)
                projectFilter = null
                renderMeetings()
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    private fun transcribeSelected() {
        val ids = selection.toList()
        clearSelection()
        // Сервис ставит работы в очередь сам — просто отдаём ему все разом.
        for (id in ids) {
            val meeting = MeetingStore.load(this, id) ?: continue
            if (!meeting.isDone && id != MeetingService.recordingId) {
                MeetingService.transcribe(this, id)
            }
        }
        renderMeetings()
    }

    /**
     * Экспорт выбранных встреч: формат, потом — отдельными файлами или
     * одним общим. Расшифровки нет — экспортировать нечего, такие пропускаются.
     */
    private fun exportSelected() {
        val picked = selection.toList()
            .mapNotNull { MeetingStore.load(this, it) }
            .sortedBy { it.at }
        val formats = listOf(
            "txt" to getString(R.string.export_txt),
            "md" to getString(R.string.export_md),
            "pdf" to getString(R.string.export_pdf),
            "docx" to getString(R.string.export_doc),
            "wav" to getString(R.string.export_wav),
        )
        optionSheet(getString(R.string.selection_export), formats, null) { value ->
            // Звук есть и у нерасшифрованных, и склеивать его не во что —
            // всегда отдельными файлами, без второго вопроса.
            if (value == "wav") {
                val withAudio = picked.filter { MeetingStore.audioFile(this, it.id).exists() }
                if (withAudio.isEmpty()) {
                    Snackbar.make(ui.root, R.string.selection_audio_none, Snackbar.LENGTH_LONG)
                        .show()
                } else {
                    runAudioExport(withAudio)
                }
                return@optionSheet
            }
            val done = picked.filter { it.isDone }
            if (done.isEmpty()) {
                Snackbar.make(ui.root, R.string.selection_export_none, Snackbar.LENGTH_LONG)
                    .show()
                return@optionSheet
            }
            val format = when (value) {
                "txt" -> ExportFormat.TXT
                "md" -> ExportFormat.MD
                "pdf" -> ExportFormat.PDF
                else -> ExportFormat.DOCX
            }
            if (done.size == 1) {
                runExport(done, format, combined = false)
            } else {
                optionSheet(
                    getString(R.string.export_how),
                    listOf(
                        "separate" to getString(R.string.export_separate),
                        "single" to getString(R.string.export_single),
                    ),
                    null,
                ) { how -> runExport(done, format, combined = how == "single") }
            }
        }
    }

    private fun runAudioExport(meetings: List<Meeting>) {
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    meetings.map { MeetingExport.saveAudio(this@MainActivity, it) }
                }
            }
            result.fold(
                onSuccess = { exports ->
                    clearSelection()
                    Snackbar.make(
                        ui.root,
                        if (exports.size == 1) {
                            getString(R.string.export_done, exports.first().name)
                        } else {
                            getString(R.string.export_done_many, exports.size)
                        },
                        Snackbar.LENGTH_LONG,
                    ).show()
                },
                onFailure = {
                    Snackbar.make(ui.root, R.string.export_failed, Snackbar.LENGTH_LONG).show()
                },
            )
        }
    }

    private fun runExport(meetings: List<Meeting>, format: ExportFormat, combined: Boolean) {
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val loaded = meetings.map {
                        it to MeetingStore.loadTranscript(this@MainActivity, it.id)
                    }
                    if (combined) {
                        listOf(MeetingExport.saveCombined(this@MainActivity, loaded, format))
                    } else {
                        loaded.map { (m, segments) ->
                            MeetingExport.save(this@MainActivity, m, segments, format)
                        }
                    }
                }
            }
            result.fold(
                onSuccess = { exports ->
                    clearSelection()
                    Snackbar.make(
                        ui.root,
                        if (exports.size == 1) {
                            getString(R.string.export_done, exports.first().name)
                        } else {
                            getString(R.string.export_done_many, exports.size)
                        },
                        Snackbar.LENGTH_LONG,
                    ).show()
                },
                onFailure = {
                    Snackbar.make(ui.root, R.string.export_failed, Snackbar.LENGTH_LONG).show()
                },
            )
        }
    }

    /** Markdown-метки саммери — в читаемый текст с точками и жирными темами. */
    private fun renderSummaryText(summary: String): CharSequence {
        val builder = android.text.SpannableStringBuilder()
        for (raw in summary.lines()) {
            val line = raw.trim()
            if (line.isEmpty()) continue
            if (builder.isNotEmpty()) builder.append("\n")
            when {
                line.startsWith("#") -> {
                    val text = line.trimStart('#').trim()
                    if (builder.isNotEmpty()) builder.append("\n")
                    val start = builder.length
                    builder.append(text)
                    builder.setSpan(
                        android.text.style.StyleSpan(android.graphics.Typeface.BOLD),
                        start, builder.length,
                        android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
                    )
                }
                line.startsWith("- ") || line.startsWith("• ") ->
                    builder.append("•  ").append(line.substring(2).trim())
                else -> builder.append(line)
            }
        }
        return builder
    }

    /** WAV открытой встречи — в Загрузки, тем же снекбаром, что экспорт. */
    private fun saveMeetingAudio() {
        val meeting = openMeetingId?.let { MeetingStore.load(this, it) } ?: return
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { MeetingExport.saveAudio(this@MainActivity, meeting) }
            }
            result.fold(
                onSuccess = { export ->
                    Snackbar.make(
                        ui.root,
                        getString(R.string.export_done, export.name),
                        Snackbar.LENGTH_LONG,
                    ).show()
                },
                onFailure = {
                    Snackbar.make(ui.root, R.string.export_failed, Snackbar.LENGTH_LONG).show()
                },
            )
        }
    }

    private fun deleteSelected() {
        val ids = selection.toList()
        if (ids.isEmpty()) return
        MaterialAlertDialogBuilder(this)
            .setTitle(getString(R.string.selection_delete))
            .setMessage(getString(R.string.selection_delete_message, ids.size))
            .setPositiveButton(R.string.meeting_delete) { _, _ ->
                for (id in ids) {
                    if (id == MeetingService.recordingId) continue
                    if (openMeetingId == id) openMeetingId = null
                    MeetingStore.delete(this, id)
                }
                clearSelection()
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    /**
     * Расшифровка по ссылке. Поле заранее заполняется буфером обмена: ссылку
     * почти всегда копируют перед тем, как прийти сюда.
     */
    private fun askLink() {
        val clip = (getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager)
            .primaryClip?.getItemAt(0)?.text?.toString()?.trim().orEmpty()
        val input = EditText(this).apply {
            hint = getString(R.string.link_hint)
            inputType = android.text.InputType.TYPE_TEXT_VARIATION_URI
            if (clip.startsWith("http")) setText(clip)
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.link_dialog_title)
            .setMessage(R.string.link_dialog_message)
            .setView(input)
            .setPositiveButton(R.string.link_go) { _, _ ->
                startLink(input.text?.toString()?.trim().orEmpty())
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    private fun startLink(url: String) {
        val problem = LinkImport.unsupportedReason(this, url)
        if (problem != null) {
            Snackbar.make(ui.root, problem, Snackbar.LENGTH_LONG).show()
            return
        }
        MeetingService.importUrl(this, url)
        show(Page.MEETINGS)
        Snackbar.make(ui.root, R.string.link_started, Snackbar.LENGTH_SHORT).show()
    }

    private fun toggleMeetingRecording() {
        haptic(ui.pageMeetings.meetingRecord)
        if (MeetingService.recordingId != null) {
            MeetingService.stopRecording(this)
        } else if (hasMic()) {
            MeetingService.startRecording(this)
        } else {
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        }
        renderMeetings()
    }

    private var meetingPulse: Animator? = null
    private var meetingWaveJob: Job? = null
    private var detailShown = false

    /** Пульс и волна живут, только пока реально идёт запись. */
    private fun syncMeetingRecordingMotion(recording: Boolean) {
        val m = ui.pageMeetings
        if (recording) {
            if (meetingPulse == null && motionOn()) {
                meetingPulse = startPulse(m.recordPulse)
            }
            if (meetingWaveJob == null) {
                meetingWaveJob = lifecycleScope.launch {
                    while (true) {
                        m.meetingWave.push(MeetingService.recordingLevel)
                        delay(WAVE_TICK_MS)
                    }
                }
            }
        } else {
            stopPulse(meetingPulse, m.recordPulse)
            meetingPulse = null
            meetingWaveJob?.cancel()
            meetingWaveJob = null
            m.meetingWave.reset()
        }
    }

    private fun renderMeetings() {
        val m = ui.pageMeetings
        val open = openMeetingId?.let { MeetingStore.load(this, it) }

        // Список и деталь меняются местами с лёгким сдвигом — направление
        // подсказывает, куда пользователь «пошёл».
        val showDetail = open != null
        if (showDetail != detailShown) {
            detailShown = showDetail
            val enter = if (showDetail) m.meetingDetail else m.meetingsHome
            val exit = if (showDetail) m.meetingsHome else m.meetingDetail
            exit.animate().cancel()
            exit.visibility = View.GONE
            enter.visibility = View.VISIBLE
            if (motionOn()) {
                enter.alpha = 0f
                enter.translationX =
                    (if (showDetail) 24 else -24) * resources.displayMetrics.density
                enter.animate().alpha(1f).translationX(0f)
                    .setDuration(animBase)
                    .setInterpolator(DecelerateInterpolator())
                    .start()
            }
        } else {
            m.meetingsHome.visibility = visibility(!showDetail)
            m.meetingDetail.visibility = visibility(showDetail)
        }

        if (open == null) {
            openMeetingId = null
            // Ушли из встречи — поиск по её тексту больше не нужен, а
            // таймлайн при следующем открытии нужно перечитать заново.
            if (!m.meetingFind.text.isNullOrEmpty()) m.meetingFind.setText("")
            detailSegments = emptyList()
            shownTranscript = null
            val recording = MeetingService.recordingId != null
            val paused = MeetingService.recordingPaused
            m.meetingRecord.setIconResource(
                if (recording) R.drawable.ic_stop else R.drawable.ic_mic
            )
            m.meetingRecord.contentDescription =
                getString(if (recording) R.string.meeting_stop else R.string.meeting_record)
            // На время записи импорт уступает место паузе и «глушить фон»:
            // загружать файлы всё равно нельзя, пока микрофон занят.
            m.meetingImport.visibility = visibility(!recording)
            m.meetingLink.visibility = visibility(!recording)
            m.meetingPause.visibility = visibility(recording)
            m.meetingPause.setText(
                if (paused) R.string.meeting_resume else R.string.meeting_pause
            )
            m.meetingSuppressRow.visibility = visibility(recording)
            if (m.meetingSuppressSwitch.isChecked != MeetingService.suppressNoise) {
                m.meetingSuppressSwitch.isChecked = MeetingService.suppressNoise
            }
            m.meetingStatus.text = getString(
                when {
                    paused -> R.string.meeting_state_paused
                    recording -> R.string.meeting_state_recording
                    else -> R.string.meeting_idle
                }
            )
            m.meetingTimer.visibility = visibility(recording)
            m.meetingWave.visibility = visibility(recording)
            if (recording) {
                m.meetingTimer.text =
                    MeetingStore.clockLabel(MeetingService.recordingSeconds.toFloat())
            }
            syncMeetingRecordingMotion(recording && !paused)

            renderMeetingList()
            return
        }
        syncMeetingRecordingMotion(false)

        m.meetingTitle.text = MeetingStore.displayTitle(this, open)
        m.meetingMeta.text = getString(
            R.string.meeting_meta,
            MeetingStore.durationLabel(this, open.seconds),
            SimpleDateFormat("d MMMM yyyy, HH:mm", Locale("ru")).format(Date(open.at)),
        )

        val percent = MeetingService.progress[open.id]
        val phaseRes = MeetingService.phase[open.id]
        val working = percent != null && phaseRes != null
        m.meetingState.visibility = visibility(!open.isDone || working)
        m.meetingState.text = when {
            working -> getString(phaseRes!!, percent)
            open.state == Meeting.STATE_FAILED -> getString(R.string.meeting_state_failed)
            open.isDone -> ""
            else -> getString(R.string.meeting_state_recorded)
        }
        m.meetingTranscribe.visibility = visibility(
            !working && !open.isDone && open.id != MeetingService.recordingId
        )
        m.meetingProgress.visibility = visibility(working)
        m.meetingProgress.setProgress(percent ?: 0, working && motionOn())
        m.meetingCancelWork.visibility = visibility(working)

        // Карточка саммери показывается, если оно есть у встречи: на
        // телефоне саммери не считается — его принесёт синхронизация.
        m.meetingSummaryBox.visibility = visibility(open.summary.isNotEmpty())
        if (open.summary.isNotEmpty()) {
            val rendered = renderSummaryText(open.summary)
            if (m.meetingSummaryText.text.toString() != rendered.toString()) {
                m.meetingSummaryText.text = rendered
            }
            // Потолок высоты: длинное саммери листается внутри карточки,
            // а таймлайн и кнопки экспорта остаются достижимыми.
            val cap = (300 * resources.displayMetrics.density).toInt()
            m.meetingSummaryScroll.post {
                val wanted = if (m.meetingSummaryText.height > cap) cap
                else ViewGroup.LayoutParams.WRAP_CONTENT
                if (m.meetingSummaryScroll.layoutParams.height != wanted) {
                    m.meetingSummaryScroll.layoutParams =
                        m.meetingSummaryScroll.layoutParams.apply { height = wanted }
                }
            }
        }
        m.meetingDiarize.visibility = visibility(open.isDone && !working)
        m.meetingExportRow.visibility = visibility(open.isDone)
        m.meetingShareText.visibility = visibility(open.isDone)
        // Звук есть и у нерасшифрованной встречи — лишь бы работа по ней
        // не шла и файл был на месте.
        m.meetingSaveAudio.visibility = visibility(
            !working && open.id != MeetingService.recordingId &&
                MeetingStore.audioFile(this, open.id).exists()
        )
        m.meetingCopy.visibility = visibility(open.isDone)
        // Пока по встрече идёт работа, удалять её из-под неё нельзя.
        m.meetingDelete.visibility = visibility(!working)

        segments.setLabels(
            (0 until open.speakers).associateWith {
                MeetingStore.speakerLabel(this, open, it)
            }
        )

        // Таймлайн перечитывается только когда мог измениться: JSON у
        // двухчасовой встречи немаленький, а перерисовка идёт каждые 600 мс.
        val key = open.id to open.state + (percent ?: -1) + "s${open.speakers}"
        if (shownTranscript != key) {
            // Открыли другую встречу — RecyclerView хранит прокрутку старой.
            val switched = shownTranscript?.first != open.id
            shownTranscript = key
            detailSegments = MeetingStore.loadTranscript(this, open.id)
            segments.submit(detailSegments)
            if (switched) {
                m.meetingTimeline.scrollToPosition(0)
                stagger(m.meetingTimeline)
            }
        }

        // Поиск по расшифровке появляется, когда есть что искать.
        m.meetingFindRow.visibility = visibility(detailSegments.isNotEmpty())

        // Пришли из общего поиска — подставляем слово и ведём к месту.
        val wanted = pendingFind
        if (wanted != null) {
            pendingFind = null
            val index = pendingIndex
            pendingIndex = null
            if (m.meetingFind.text?.toString() != wanted) {
                // Слушатель поля сам пересчитает совпадения.
                m.meetingFind.setText(wanted)
            } else {
                applyFind(jump = index == null)
            }
            index?.let { jumpToSegment(it) }
            if (index != null) {
                findIndex = findMatches.indexOf(index)
                m.meetingFindCount.text = if (findIndex >= 0) {
                    getString(R.string.find_position, findIndex + 1, findMatches.size)
                } else {
                    m.meetingFindCount.text
                }
            }
        }
    }

    /**
     * Список встреч с учётом поиска, папки и режима выбора.
     *
     * Поиск и папка прячутся, пока встреч нет вовсе: пустой экран не должен
     * предлагать фильтровать пустоту.
     */
    private fun renderMeetingList() {
        val m = ui.pageMeetings
        val all = MeetingStore.all(this)
        // Отметки могли устареть: встречу удалили пачкой или из детали.
        selection.retainAll(all.map { it.id }.toSet())

        val query = meetingQuery.trim()
        val hits = if (query.isBlank()) emptyList() else MeetingStore.search(this, query)
        val found = if (query.isBlank()) all else hits.map { it.meeting }
        val items = when (projectFilter) {
            null -> found
            NO_PROJECT -> found.filter { it.project == null }
            else -> found.filter { it.project == projectFilter }
        }
        val hitsById = hits.associateBy { it.meeting.id }

        // Список перечитывается каждые 600 мс, пока вкладка открыта, но
        // отдавать его адаптеру заново нужно, только если что-то изменилось:
        // notifyDataSetChanged пересобирает карточки и отменяет начатое на
        // них долгое нажатие — режим выбора просто не успевал включиться.
        val key = buildString {
            for (item in items) {
                append(item.id).append(item.state).append(item.speakers)
                append(item.project).append(MeetingService.progress[item.id]).append(';')
            }
            append('|').append(selection.sorted().joinToString(","))
            append('|').append(query)
            append('|').append(hitsById.keys.sorted().joinToString(","))
        }
        if (key != shownMeetingList) {
            shownMeetingList = key
            meetings.selected = selection.toSet()
            meetings.submit(items, hitsById, query)
        }

        val selecting = selection.isNotEmpty()
        // Во время записи кнопка «стоп» остаётся на месте даже в режиме
        // выбора: спрятать единственный способ остановить запись нельзя.
        m.meetingTools.visibility =
            visibility(!selecting || MeetingService.recordingId != null)
        m.selectionPanel.visibility = visibility(selecting)
        m.selectionCount.text = getString(R.string.selected_count, selection.size)
        // Панель забирает место у кнопки записи — появление подсказывает, что
        // это смена состояния экрана, а не подмена интерфейса рывком.
        if (selecting != selectionShown) {
            selectionShown = selecting
            if (selecting) enterPage(m.selectionPanel)
        }

        val hasMeetings = all.isNotEmpty()
        m.meetingSearch.visibility = visibility(hasMeetings && !selecting)
        m.meetingFilters.visibility = visibility(hasMeetings && !selecting)
        m.filterProject.text = buildString {
            append(
                when (projectFilter) {
                    null -> getString(R.string.project_all)
                    NO_PROJECT -> getString(R.string.project_none)
                    else -> MeetingStore.projectName(this@MainActivity, projectFilter)
                        ?: getString(R.string.project_all)
                }
            )
            append(" ▾")
        }
        // Подчёркивание — только у включённого фильтра, как в каталоге.
        m.filterProject.setBackgroundResource(
            if (projectFilter != null) R.drawable.bg_tab_active else 0
        )
        m.searchSummary.text = when {
            query.isBlank() -> ""
            items.isEmpty() -> getString(R.string.search_nothing)
            else -> getString(R.string.search_hits, items.size)
        }

        m.meetingsEmpty.visibility =
            visibility(items.isEmpty() && query.isBlank() && projectFilter == null)
    }

    /**
     * Число говорящих сильно помогает кластеризации — спрашиваем всегда.
     * Нижний лист вместо центрального диалога: варианты под пальцем.
     */
    private fun chooseDiarize() {
        val id = openMeetingId ?: return
        val sheet = BottomSheetDialog(this)
        val view = layoutInflater.inflate(R.layout.sheet_speakers, null)
        view.findViewById<TextView>(R.id.sheetHint).visibility =
            visibility(!Diarizer.modelsReady(this))

        val box = view.findViewById<LinearLayout>(R.id.sheetOptions)
        val options = listOf(
            R.string.diarize_auto to 0,
            R.string.diarize_two to 2,
            R.string.diarize_three to 3,
            R.string.diarize_four to 4,
            R.string.diarize_five to 5,
        )
        for ((title, count) in options) {
            val row = layoutInflater
                .inflate(R.layout.item_sheet_option, box, false) as TextView
            row.setText(title)
            row.setOnClickListener {
                sheet.dismiss()
                MeetingService.diarize(this, id, count)
                renderMeetings()
            }
            box.addView(row)
        }

        sheet.setContentView(view)
        // Фон рисует наша разметка со скруглённым верхом, контейнер — прозрачный.
        (view.parent as? View)?.setBackgroundColor(0)
        view.setBackgroundResource(R.drawable.bg_sheet)
        sheet.show()
    }

    /** Тап по заголовку говорящего в таймлайне — дать человеку имя. */
    private fun renameSpeaker(speaker: Int) {
        val meeting = openMeetingId?.let { MeetingStore.load(this, it) } ?: return
        val input = EditText(this).apply {
            setText(meeting.speakerNames[speaker].orEmpty())
            hint = getString(R.string.speaker_rename_hint)
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.speaker_rename_title)
            .setView(input)
            .setPositiveButton(R.string.save) { _, _ ->
                val name = input.text.toString().trim()
                val names = meeting.speakerNames.toMutableMap()
                if (name.isEmpty()) names.remove(speaker) else names[speaker] = name
                MeetingStore.save(this, meeting.copy(speakerNames = names))
                renderMeetings()
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    private fun renameMeeting() {
        val meeting = openMeetingId?.let { MeetingStore.load(this, it) } ?: return
        val input = EditText(this).apply {
            setText(meeting.title.ifBlank { MeetingStore.displayTitle(context, meeting) })
            setSelectAllOnFocus(true)
        }
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.meeting_rename_title)
            .setView(input)
            .setPositiveButton(R.string.save) { _, _ ->
                MeetingStore.save(this, meeting.copy(title = input.text.toString().trim()))
                renderMeetings()
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    /**
     * Текст встречи в системный лист «Поделиться»: переслать человеку или
     * закинуть во внешнюю нейросеть — осознанный шаг пользователя, само
     * приложение в интернет ничего не отправляет.
     */
    private fun shareMeetingText() {
        val meeting = openMeetingId?.let { MeetingStore.load(this, it) } ?: return
        val text = buildString {
            appendLine(MeetingStore.displayTitle(this@MainActivity, meeting))
            if (meeting.summary.isNotEmpty()) {
                appendLine()
                appendLine(meeting.summary.trim())
            }
            appendLine()
            var previous: Int? = null
            for (s in MeetingStore.loadTranscript(this@MainActivity, meeting.id)) {
                if (s.speaker != null && s.speaker != previous) {
                    appendLine(MeetingStore.speakerLabel(this@MainActivity, meeting, s.speaker))
                }
                previous = s.speaker
                appendLine("${MeetingStore.clockLabel(s.start)}  ${s.text}")
            }
        }.trimEnd()
        if (text.isBlank()) return
        startActivity(
            Intent.createChooser(
                Intent(Intent.ACTION_SEND)
                    .setType("text/plain")
                    .putExtra(Intent.EXTRA_TEXT, text),
                getString(R.string.meeting_share_text),
            )
        )
    }

    private fun copyMeeting() {
        val meeting = openMeetingId?.let { MeetingStore.load(this, it) } ?: return
        val text = buildString {
            var previous: Int? = null
            for (s in MeetingStore.loadTranscript(this@MainActivity, meeting.id)) {
                if (s.speaker != null && s.speaker != previous) {
                    if (isNotEmpty()) appendLine()
                    appendLine(MeetingStore.speakerLabel(this@MainActivity, meeting, s.speaker))
                }
                previous = s.speaker
                appendLine("${MeetingStore.clockLabel(s.start)}  ${s.text}")
            }
        }.trimEnd()
        copyToClipboard(text)
    }

    private fun deleteMeeting() {
        val id = openMeetingId ?: return
        MaterialAlertDialogBuilder(this)
            .setMessage(R.string.meeting_delete_confirm)
            .setPositiveButton(R.string.meeting_delete) { _, _ ->
                MeetingStore.delete(this, id)
                openMeetingId = null
                renderMeetings()
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    private fun exportMeeting(format: ExportFormat) {
        val meeting = openMeetingId?.let { MeetingStore.load(this, it) } ?: return
        lifecycleScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    MeetingExport.save(
                        this@MainActivity, meeting,
                        MeetingStore.loadTranscript(this@MainActivity, meeting.id),
                        format,
                    )
                }
            }
            result.fold(
                onSuccess = { export ->
                    val bar = Snackbar.make(
                        ui.root,
                        getString(R.string.export_done, export.name),
                        Snackbar.LENGTH_LONG,
                    )
                    if (export.uri != null) {
                        bar.setAction(R.string.export_open) {
                            runCatching {
                                startActivity(
                                    Intent(Intent.ACTION_VIEW)
                                        .setDataAndType(export.uri, format.mime)
                                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                )
                            }
                        }
                    }
                    bar.show()
                },
                onFailure = {
                    Snackbar.make(ui.root, R.string.export_failed, Snackbar.LENGTH_LONG).show()
                },
            )
        }
    }

    // --- разрешения и плавающая кнопка ------------------------------------

    /** granted == null — состояние узнать нечем (фирменный автозапуск). */
    private data class Permission(
        val title: Int,
        val hint: CharSequence,
        val granted: Boolean?,
        val request: () -> Unit,
    )

    private fun permissions(): List<Permission> = buildList {
        add(Permission(R.string.perm_mic, getString(R.string.perm_mic_hint), hasMic()) {
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        })
        add(Permission(R.string.perm_overlay, getString(R.string.perm_overlay_hint), hasOverlay()) {
            startActivity(
                Intent(
                    Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                    Uri.parse("package:$packageName"),
                )
            )
        })
        add(
            Permission(
                R.string.perm_accessibility,
                getString(R.string.perm_accessibility_hint),
                HandyAccessibilityService.isGranted(this@MainActivity),
            ) { startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            add(Permission(R.string.perm_notifications, getString(R.string.perm_notifications_hint), hasNotifications()) {
                askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
            })
        }
        // Без снятия ограничений система через какое-то время выгружает
        // приложение вместе с кнопкой — именно так она и «пропадала сама».
        add(Permission(R.string.perm_battery, getString(R.string.perm_battery_hint), ignoresBatteryOptimizations()) {
            runCatching {
                startActivity(
                    Intent(
                        Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                        Uri.parse("package:$packageName"),
                    )
                )
            }
        })
        // Автозапуск живёт в фирменной оболочке, и прочитать его состояние
        // системными средствами нельзя — поэтому просто ведём в нужный экран.
        add(
            Permission(
                R.string.perm_autostart,
                getString(R.string.perm_autostart_hint, deviceMaker()),
                null,
            ) { openAutostartScreen() }
        )
    }

    /**
     * Имя производителя для текста карточки. У каждой оболочки свой экран
     * автозапуска и свои правила, поэтому подставляем то, что написано на
     * самом телефоне, а не зашитый бренд.
     */
    private fun deviceMaker(): String {
        val maker = Build.MANUFACTURER?.trim().orEmpty()
        if (maker.isEmpty()) return getString(R.string.this_phone)
        return maker.replaceFirstChar { it.uppercase() }
    }

    /**
     * Фирменные экраны автозапуска разных оболочек. Проверяем по очереди и
     * открываем первый существующий; если своего экрана нет (так у Samsung и
     * у чистого Android), ведём в настройки приложения, где лежат ограничения
     * фоновой работы.
     */
    private fun autostartCandidates() = listOf(
        // Xiaomi, Redmi, POCO
        "com.miui.securitycenter" to "com.miui.permcenter.autostart.AutoStartManagementActivity",
        // Samsung: раздел ухода за батареей
        "com.samsung.android.lool" to "com.samsung.android.sm.ui.battery.BatteryActivity",
        // Huawei, Honor
        "com.huawei.systemmanager" to "com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity",
        // Oppo, Realme
        "com.coloros.safecenter" to "com.coloros.safecenter.permission.startup.StartupAppListActivity",
        // Vivo
        "com.vivo.permissionmanager" to "com.vivo.permissionmanager.activity.BgStartUpManagerActivity",
        // OnePlus
        "com.oneplus.security" to "com.oneplus.security.chainlaunch.view.ChainLaunchAppListActivity",
    )

    private fun openAutostartScreen() {
        for ((pkg, activity) in autostartCandidates()) {
            val intent = Intent().setComponent(ComponentName(pkg, activity))
            if (packageManager.resolveActivity(intent, 0) != null) {
                if (runCatching { startActivity(intent) }.isSuccess) return
            }
        }
        runCatching {
            startActivity(
                Intent(
                    Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                    Uri.parse("package:$packageName"),
                )
            )
        }
    }

    private fun ignoresBatteryOptimizations(): Boolean =
        (getSystemService(Context.POWER_SERVICE) as PowerManager)
            .isIgnoringBatteryOptimizations(packageName)

    private fun renderPermissions() {
        val items = permissions()
        // Автозапуск в счёт не идёт: его состояние оболочка не сообщает,
        // иначе список никогда бы не свернулся.
        val allGranted = items.none { it.granted == false }

        val summary = ui.pageDictation.permissionsSummary
        summary.visibility = visibility(allGranted)
        summary.setText(R.string.perm_all_done)
        summary.setOnClickListener {
            permissionsExpanded = !permissionsExpanded
            renderPermissions()
        }

        val container = ui.pageDictation.permissions
        container.visibility = visibility(!allGranted || permissionsExpanded)
        container.removeAllViews()
        if (allGranted && !permissionsExpanded) {
            renderBubbleToggle()
            return
        }

        for (p in items) {
            val row = LayoutInflater.from(this)
                .inflate(R.layout.item_permission, container, false)
            row.findViewById<TextView>(R.id.title).setText(p.title)
            row.findViewById<TextView>(R.id.hint).text = p.hint
            val granted = p.granted == true
            row.findViewById<View>(R.id.bullet).visibility = visibility(granted)
            row.findViewById<TextView>(R.id.done).visibility = visibility(granted)
            row.findViewById<MaterialButton>(R.id.grant).apply {
                visibility = visibility(!granted)
                setText(if (p.granted == null) R.string.perm_open else R.string.perm_grant)
                setOnClickListener { p.request() }
            }
            container.addView(row)
        }
        renderBubbleToggle()
    }

    private fun renderBubbleToggle() {
        val running = DictationService.running
        ui.pageDictation.bubbleToggle.setText(
            if (running) R.string.bubble_disable else R.string.bubble_enable
        )
        // Без показа поверх окон кнопку просто негде рисовать.
        ui.pageDictation.bubbleToggle.isEnabled = running || (hasMic() && hasOverlay())

        // Кнопка работает, но текст идёт мимо поля — это надо показать заметно,
        // иначе выглядит как поломка распознавания.
        val degraded = running && !HandyAccessibilityService.isGranted(this)
        ui.pageDictation.degradedWarning.visibility = visibility(degraded)
    }

    private fun toggleBubble() {
        if (DictationService.running) {
            DictationService.stop(this)
        } else {
            DictationService.start(this)
        }
        // Состояние подхватит watchState — отдельная задержка не нужна.
    }

    private fun hasMic() = ContextCompat.checkSelfPermission(
        this, Manifest.permission.RECORD_AUDIO
    ) == PackageManager.PERMISSION_GRANTED

    private fun hasOverlay() = Settings.canDrawOverlays(this)

    private fun hasNotifications() =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(
                this, Manifest.permission.POST_NOTIFICATIONS
            ) == PackageManager.PERMISSION_GRANTED

    // --- история ---------------------------------------------------------

    private fun setupHistory() {
        history = HistoryAdapter(
            onTap = { copyToClipboard(it.text) },
            onPlay = { togglePlayback(it) },
            onRetry = { retranscribe(it) },
            onDelete = { deleteTranscript(it) },
        )
        ui.pageHistory.historyList.layoutManager = LinearLayoutManager(this)
        ui.pageHistory.historyList.adapter = history
        ui.pageHistory.historyClear.setOnClickListener {
            stopPlayback()
            TranscriptStore.clear(this)
            refreshHistory()
        }
    }

    /**
     * Плеер один на весь список: играть две диктовки одновременно незачем,
     * а звук, оставшийся за свёрнутым экраном, — верный способ напугать.
     */
    private fun togglePlayback(item: Transcript) {
        if (history.playingAt == item.at) {
            stopPlayback()
            return
        }
        val file = TranscriptStore.audioFile(this, item.at)
        if (!file.exists()) {
            Snackbar.make(ui.root, R.string.history_no_audio, Snackbar.LENGTH_SHORT).show()
            return
        }
        stopPlayback()
        val started = runCatching {
            player = MediaPlayer().apply {
                setDataSource(file.absolutePath)
                setOnCompletionListener { stopPlayback() }
                prepare()
                start()
            }
        }.isSuccess
        history.playingAt = if (started) item.at else null
    }

    private fun stopPlayback() {
        player?.let {
            runCatching { it.stop() }
            it.release()
        }
        player = null
        history.playingAt = null
    }

    /**
     * Расшифровать заново — тем, что стоит активной моделью сейчас. Смысл в
     * этом и есть: быстрая модель ошиблась, ставим точную и переспрашиваем.
     */
    private fun retranscribe(item: Transcript) {
        val file = TranscriptStore.audioFile(this, item.at)
        if (!file.exists()) {
            Snackbar.make(ui.root, R.string.history_no_audio, Snackbar.LENGTH_SHORT).show()
            return
        }
        stopPlayback()
        val bar = Snackbar.make(ui.root, R.string.history_retrying, Snackbar.LENGTH_INDEFINITE)
        bar.show()
        lifecycleScope.launch {
            val pcm = withContext(Dispatchers.IO) {
                WavReader(file).use { it.read(0, it.totalSamples.toInt()) }
            }
            val ready = Engine.ensureLoaded(this@MainActivity)
            val text = if (!ready) "" else withContext(Dispatchers.Default) {
                Engine.transcribe(
                    pcm,
                    AudioRecorder.SAMPLE_RATE,
                    AppPrefs.removeFillers(this@MainActivity),
                )
            }
            Engine.scheduleUnload(this@MainActivity)
            bar.dismiss()
            if (text.isBlank()) {
                Snackbar.make(
                    ui.root,
                    if (ready) R.string.nothing_heard else R.string.no_model,
                    Snackbar.LENGTH_SHORT,
                ).show()
                return@launch
            }
            TranscriptStore.updateText(this@MainActivity, item.at, text)
            refreshHistory()
            Snackbar.make(
                ui.root,
                getString(R.string.history_retried, text.take(60)),
                Snackbar.LENGTH_SHORT,
            ).show()
        }
    }

    private fun deleteTranscript(item: Transcript) {
        if (history.playingAt == item.at) stopPlayback()
        TranscriptStore.remove(this, item.at)
        refreshHistory()
    }

    private fun refreshHistory() {
        val items = TranscriptStore.all(this)
        history.submit(this, items)
        ui.pageHistory.historyEmpty.visibility = visibility(items.isEmpty())
        ui.pageHistory.historyClear.visibility = visibility(items.isNotEmpty())
    }

    // --- каталог моделей --------------------------------------------------

    private fun setupModels() {
        models = ModelAdapter(::onModelTapped) { _, file ->
            // Переключение версии прямо из списка, без окна загрузки.
            ModelStore.setActive(this, file.filename)
            refreshModels()
            prepareEngine()
        }
        ui.pageModels.modelList.layoutManager = LinearLayoutManager(this)
        ui.pageModels.modelList.adapter = models

        // Отмена всех идущих загрузок прямо со страницы, не только из шторки.
        ui.pageModels.progressCancel.setOnClickListener {
            DownloadService.progress.keys.toList().forEach {
                DownloadService.cancel(this, it)
            }
            Toast.makeText(this, R.string.download_cancelled, Toast.LENGTH_SHORT).show()
            renderDownloadSummary()
        }
        ui.pageModels.filterLanguage.setOnClickListener { chooseLanguage() }
        ui.pageModels.filterDownloaded.setOnClickListener {
            onlyDownloaded = !onlyDownloaded
            renderFilters()
        }
        clearableSearch(ui.pageModels.modelSearch)
        ui.pageModels.modelSearch.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable?) {
                modelQuery = s?.toString().orEmpty()
                refreshModels()
            }
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        renderFilters()
    }

    private fun renderFilters() {
        val language = Catalog.languages(this).firstOrNull { it.code == languageFilter }
        ui.pageModels.filterLanguage.text = buildString {
            append(language?.name?.replaceFirstChar { it.uppercase() }
                ?: getString(R.string.filter_all_languages))
            append(" ▾")
        }
        ui.pageModels.filterLanguage.setTextColor(getColor(R.color.ink))
        // Подчёркивание — только у включённого фильтра, как в меню на сайте.
        ui.pageModels.filterLanguage.setBackgroundResource(
            if (languageFilter != null) R.drawable.bg_tab_active else 0
        )

        ui.pageModels.filterDownloaded.setTextColor(
            getColor(if (onlyDownloaded) R.color.ink else R.color.fog)
        )
        ui.pageModels.filterDownloaded.setBackgroundResource(
            if (onlyDownloaded) R.drawable.bg_tab_active else 0
        )
        refreshModels()
    }

    /** Список языков с поиском: их больше сотни, вкладками не обойтись. */
    private fun chooseLanguage() {
        val view = layoutInflater.inflate(R.layout.dialog_languages, null)
        val list = view.findViewById<LinearLayout>(R.id.languageList)
        val search = view.findViewById<EditText>(R.id.languageSearch)
        val dialog = MaterialAlertDialogBuilder(this)
            .setTitle(R.string.choose_language)
            .setView(view)
            .setNegativeButton(R.string.close, null)
            .create()

        fun addRow(title: String, count: Int?, code: String?) {
            val row = layoutInflater.inflate(R.layout.item_language, list, false)
            row.findViewById<TextView>(R.id.language).text =
                title.replaceFirstChar { it.uppercase() }
            row.findViewById<TextView>(R.id.count).text = count?.toString().orEmpty()
            row.findViewById<View>(R.id.bullet).visibility =
                if (code == languageFilter) View.VISIBLE else View.INVISIBLE
            row.setOnClickListener {
                languageFilter = code
                renderFilters()
                dialog.dismiss()
            }
            list.addView(row)
        }

        fun render(query: String) {
            list.removeAllViews()
            val needle = query.trim().lowercase()
            if (needle.isEmpty()) addRow(getString(R.string.language_any), null, null)
            val matches = Catalog.languages(this)
                .filter { needle.isEmpty() || it.name.startsWith(needle) || it.code == needle }
            for (lang in matches) addRow(lang.name, lang.models, lang.code)
            if (matches.isEmpty() && needle.isNotEmpty()) {
                addRow(getString(R.string.language_nothing), null, languageFilter)
            }
        }

        render("")
        clearableSearch(search)
        search.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable?) = render(s?.toString().orEmpty())
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        dialog.show()
    }

    private fun refreshModels() {
        val downloaded = ModelStore.downloadedFilenames(this)
        val active = ModelStore.activeFilename(this)
        var shown = catalog
        val needle = modelQuery.trim().lowercase()
        if (needle.isNotEmpty()) {
            shown = shown.filter {
                it.name.lowercase().contains(needle) ||
                    it.description.lowercase().contains(needle)
            }
        }
        languageFilter?.let { code -> shown = shown.filter { code in it.languages } }
        if (onlyDownloaded) {
            shown = shown.filter { m -> m.files.any { it.filename in downloaded } }
        }

        // Когда выбран конкретный язык, одноязычные модели идут первыми:
        // обученные на одном языке точнее многоязычных на нём, хотя их общий
        // балл ниже.
        val specialisedFirst = languageFilter != null
        showModelAdvice()
        models.submit(
            shown.sortedWith(
                compareByDescending<CatalogModel> { m -> m.files.any { it.filename == active } }
                    .thenByDescending { m -> m.files.any { it.filename in downloaded } }
                    .thenByDescending { specialisedFirst && it.languageCount == 1 }
                    .thenByDescending { it.accuracyScore }
            ),
            downloaded,
            active,
        )
    }

    /**
     * Совет обычными словами: какую модель брать.
     *
     * Объясняет главное, без чего список вводит в заблуждение: баллы
     * точности считаются на общих многоязычных тестах, где русского почти
     * нет. Поэтому GigaAM со своими «69» разбирает русскую речь лучше, чем
     * модель с «90», обученная на английском.
     */
    private fun showModelAdvice() {
        val all = Catalog.models(this)
        fun bestFor(code: String) = all
            .filter { it.languageCount == 1 && code in it.languages }
            .maxByOrNull { it.accuracyScore }
        val multi = all.filter { it.languageCount > 1 }.maxByOrNull { it.accuracyScore }

        val parts = buildList {
            bestFor("ru")?.let { add(getString(R.string.model_advice_for, it.name)) }
            bestFor("en")?.let { add(getString(R.string.model_advice_en, it.name)) }
            multi?.let { add(getString(R.string.model_advice_multi, it.name)) }
        }
        ui.pageModels.modelAdvice.text =
            if (parts.isEmpty()) "" else getString(R.string.model_advice, parts.joinToString(", "))
    }

    /**
     * Версии модели списком: размер, состояние и действие у каждой строки.
     *
     * Раньше это был простой список строк, где было видно только «скачана»,
     * но не было понятно, как удалить или взять другую версию. Теперь у каждой
     * строки своя кнопка, и она меняется со «Скачать» на «Удалить».
     */
    /**
     * Версии модели списком: размер, состояние, полоса загрузки и действие.
     *
     * Загрузок может идти несколько сразу, поэтому состояние хранится по имени
     * файла, а не одной переменной: раньше вторая кнопка «Скачать» просто не
     * нажималась, пока шла первая.
     */
    private fun onModelTapped(model: CatalogModel) {
        val view = layoutInflater.inflate(R.layout.dialog_quants, null)
        val list = view.findViewById<LinearLayout>(R.id.quantList)

        fun render() {
            list.removeAllViews()
            quantRows.clear()

            val downloaded = ModelStore.downloadedFilenames(this)
            val active = ModelStore.activeFilename(this)

            for (file in model.files) {
                val row = layoutInflater.inflate(R.layout.item_quant, list, false)
                val bullet = row.findViewById<View>(R.id.bullet)
                val quant = row.findViewById<TextView>(R.id.quant)
                val note = row.findViewById<TextView>(R.id.note)
                val bar = row.findViewById<ProgressBar>(R.id.quantProgress)
                val action = row.findViewById<MaterialButton>(R.id.action)

                val isHere = file.filename in downloaded
                val isActive = file.filename == active
                val percent = DownloadService.progress[file.filename]

                quant.text = getString(R.string.quant_row, file.quant, formatSize(file.sizeBytes))
                bullet.visibility = if (isHere) View.VISIBLE else View.INVISIBLE

                note.text = when {
                    percent != null -> getString(R.string.quant_progress_note, percent)
                    isActive -> getString(R.string.quant_active_note)
                    isHere -> getString(R.string.quant_downloaded_note)
                    // Замеры GigaAM на Xiaomi 15: F16 — 20x, Q4_K_M — 11x.
                    // Квантованная версия оказалась медленнее: движок собран с
                    // поддержкой fp16, поэтому F16 считается железом напрямую,
                    // а Q4 приходится распаковывать на каждом умножении.
                    file.quant.startsWith("F") || file.quant.startsWith("BF") ->
                        getString(R.string.quant_fastest)
                    file.quant == model.defaultQuant -> getString(R.string.mirror_note)
                    file.quant == "Q4_K_M" -> getString(R.string.quant_light)
                    else -> getString(R.string.quant_middle)
                }
                note.visibility = visibility(note.text.isNotEmpty())

                bar.visibility = visibility(percent != null)
                bar.progress = percent ?: 0
                if (percent != null) quantRows[file.filename] = note to bar

                action.isEnabled = true
                action.setText(
                    when {
                        percent != null -> R.string.download_cancel
                        isHere -> R.string.quant_delete
                        else -> R.string.quant_download
                    }
                )
                action.setOnClickListener {
                    when {
                        percent != null -> cancelDownload(file)
                        isHere -> {
                            ModelStore.delete(this, file)
                            refreshModels()
                            prepareEngine()
                        }
                        else -> startDownload(model, file) { render() }
                    }
                    render()
                }

                // Тап по самой строке берёт уже скачанную версию в работу.
                row.setOnClickListener {
                    if (!isHere || isActive) return@setOnClickListener
                    ModelStore.setActive(this, file.filename)
                    refreshModels()
                    prepareEngine()
                    render()
                }

                list.addView(row)
            }
        }

        render()
        quantRender = ::render
        MaterialAlertDialogBuilder(this)
            .setTitle(model.name)
            .setView(view)
            .setNegativeButton(R.string.close, null)
            .setOnDismissListener {
                quantRows.clear()
                quantRender = null
            }
            .show()
    }

    private fun cancelDownload(file: ModelFile) {
        DownloadService.cancel(this, file.filename)
        Toast.makeText(this, R.string.download_cancelled, Toast.LENGTH_SHORT).show()
    }

    private fun startDownload(model: CatalogModel, file: ModelFile, onChanged: () -> Unit) {
        if (DownloadService.progress.containsKey(file.filename)) return
        DownloadService.start(this, model, file)
        onChanged()
    }

    /** Одна строка на все идущие загрузки: их может быть несколько сразу. */
    private fun renderDownloadSummary() {
        val label = ui.pageModels.progressLabel
        val bar = ui.pageModels.progress
        val active = DownloadService.progress.entries.toList()

        if (active.isEmpty()) {
            // Итог показывает уведомление сервиса, а эта строка живёт только
            // пока идёт загрузка. Иначе она застревала на «0%» после того,
            // как модель докачалась в фоне.
            bar.visibility = View.GONE
            ui.pageModels.progressRow.visibility = View.GONE
            return
        }
        bar.visibility = View.VISIBLE
        bar.progress = active.sumOf { it.value } / active.size
        ui.pageModels.progressRow.visibility = View.VISIBLE
        label.text = if (active.size == 1) {
            val name = Catalog.findByFilename(this, active.first().key)?.first?.name.orEmpty()
            getString(R.string.downloading, name, active.first().value)
        } else {
            getString(R.string.downloading_many, active.size)
        }
    }

    // --- прочее ----------------------------------------------------------

    private fun copyToClipboard(text: String) {
        if (text.isBlank()) return
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("transcript", text))
        Toast.makeText(this, R.string.copied, Toast.LENGTH_SHORT).show()
    }
}
