> Implemented and installed locally as 0.9.9 on 2026-09-13; not publicly released. Details below record the earlier preparation stage. See [0.9.9](release-0.9.9.md).

# Recording menu confirmation — pending release

## English

In the Mac/Windows recording detail menu, the first Delete click keeps the menu open and changes the label to Really delete?. A second click confirms deletion. Other actions (generate title and transcribe again) still run once and close the menu. Closing the menu or leaving the recording resets confirmation; the existing three-second confirmation timeout remains.

Android already opens a separate native confirmation dialog immediately after Delete, without requiring the menu to be reopened. No Android change is required for this desktop menu bug.

Verified in the isolated production UI with synthetic recordings in Russian/dark/1440, English/light/920 and Russian/light/700: first click does not delete, second click deletes once, outside click/menu toggle resets confirmation, and both other actions close the menu and invoke once. No user recordings were deleted. Prepared locally for a future release; installed/public 0.9.8 unchanged.

## Русский

В меню «Запись» на Mac/Windows первое нажатие «Удалить» оставляет меню открытым и меняет пункт на «Точно удалить?». Второе нажатие подтверждает удаление. «Придумать название» и «Расшифровать заново» по-прежнему выполняются одним нажатием и закрывают меню. Закрытие меню и уход из записи сбрасывают подтверждение; прежний таймер подтверждения в три секунды сохранён.

На Android сразу открывается отдельное системное подтверждение удаления, поэтому заново раскрывать меню не требуется. Этот дефект относится к настольному меню.

Проверено на вымышленных записях в реальном интерфейсе: русский/тёмная тема/1440, английский/светлая/920, русский/светлая/700. Первое нажатие не удаляет запись, второе удаляет один раз; закрытие сбрасывает подтверждение; оба остальных пункта выполняются один раз и закрывают меню. Реальные записи не удалялись. Подготовлено локально к следующему выпуску; установленная и опубликованная 0.9.8 не менялись.
