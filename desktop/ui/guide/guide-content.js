const GUIDE = {
  "ru": [
    {
      "nav": "Начало",
      "title": "От голоса к материалам, с которыми удобно работать",
      "lead": "Sol Flow записывает и расшифровывает речь на устройстве. Встречи можно организовать в проекты, найти нужную реплику, подготовить итоги и передать выбранные тексты внешнему ИИ.",
      "desktop": "Mac и Windows: распознавание речи и языковая модель работают локально. MCP — отдельное подключение к выбранному вами клиенту ИИ.",
      "steps": [
        "Откройте «Модели» и загрузите модель распознавания для своего языка. Загрузка требует интернета, распознавание — нет.",
        "Разрешите микрофон. Для вставки диктовки в другие приложения включите дополнительные разрешения, которые показывает экран настройки.",
        "Попробуйте короткую диктовку или импортируйте небольшую запись. Проверьте результат перед длинной встречей."
      ],
      "details": [
        [
          "Два разных ИИ",
          "Модель распознавания превращает звук в текст. Языковая модель на компьютере создает итоги и другие материалы. Внешний ChatGPT или Claude подключается отдельно через MCP."
        ],
        [
          "Что может покидать устройство",
          "Синхронизация отправляет данные в выбранное вами облако. MCP передает запрошенные разрешенные тексты клиенту ИИ. Это отдельные действия, которые вы включаете сами."
        ]
      ],
      "tip": "Это руководство доступно повторно в разделе «О проекте» → «Руководство по Sol Flow». Переключатель устройства показывает различия платформ.",
      "phone": {
        "title": "Sol Flow на телефоне",
        "lead": "Начните с вкладок внизу экрана: «Диктовка», «Встречи», «История» и «Модели». Меню ☰ открывает проекты, синхронизацию и настройки.",
        "shot": "android-dictation.png",
        "steps": [
          "В «Моделях» загрузите модель распознавания. На первом экране появится ее название.",
          "Разрешите микрофон. Для ввода в другие приложения включите плавающую кнопку и нужные разрешения.",
          "Для длинной записи перейдите во «Встречи»."
        ],
        "details": [
          [
            "Что делает телефон",
            "Записывает, расшифровывает, хранит проекты, экспортирует и синхронизирует. Саммари и карты создает компьютер. Доступом проекта к ИИ можно управлять и с телефона."
          ]
        ],
        "caption": "Большая зеленая кнопка запускает диктовку. Вкладки находятся внизу.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Диктовка",
      "title": "Надиктуйте текст в нужное приложение",
      "lead": "Диктовка подходит для сообщений, заметок и коротких фрагментов. Для длинного разговора используйте «Встречи».",
      "mac": "На Mac стандартное сочетание — ⌥ Пробел. Для вставки в другое приложение нужен доступ к универсальному доступу.",
      "windows": "На Windows проверьте назначенное сочетание в настройках. Целевое текстовое поле должно быть активно.",
      "steps": [
        "Поставьте курсор туда, где нужен текст.",
        "Нажмите кнопку или сочетание: быстрое нажатие начинает запись, повторное останавливает. Удержание — запись до отпускания.",
        "Дождитесь распознавания и проверьте вставленный текст. Предыдущие диктовки доступны в «Истории»."
      ],
      "shot": "dictation.png",
      "details": [
        [
          "Если текст не вставился",
          "Проверьте разрешения и фокус поля. Результат можно найти в истории и скопировать вручную."
        ],
        [
          "Модель и язык",
          "Русская и многоязычная речь могут требовать разных моделей. Меняйте активную модель в «Моделях» или через текущую модель."
        ]
      ],
      "tip": "Перед диктовкой убедитесь, что выбраны правильный микрофон и язык. Смена модели может занять время.",
      "phone": {
        "title": "Диктовка и плавающая кнопка",
        "lead": "На первом экране видны активная модель, кнопка микрофона и состояние разрешений.",
        "shot": "android-dictation.png",
        "steps": [
          "Нажмите зеленый микрофон, произнесите короткую фразу и нажмите еще раз.",
          "Для ввода в мессенджер откройте нужное поле и используйте плавающую кнопку Sol Flow.",
          "Если текст не вставился, найдите его во вкладке «История» и скопируйте."
        ],
        "details": [
          [
            "Разрешения",
            "Блок под микрофоном показывает, чего не хватает. После обновления проверьте специальные возможности и ограничения фоновой работы."
          ]
        ],
        "caption": "Модель выбирается над микрофоном, разрешения — в блоке ниже.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Встречи и импорт",
      "title": "Запишите разговор или принесите готовый файл",
      "lead": "Sol Flow сохраняет звук встречи в файл и создает расшифровку с таймкодами. Можно поставить запись на паузу и продолжить.",
      "desktop": "На Mac/Windows доступны аудио и видеофайлы, прямые медиассылки, Яндекс.Диск и поддерживаемые страницы YouTube, Rutube и VK.",
      "steps": [
        "Откройте «Встречи» и при необходимости выберите проект до записи или импорта.",
        "Нажмите «Записать встречу» либо выберите файл. На компьютере файл можно перетащить в окно; для ссылки есть отдельное поле.",
        "Остановите запись и дождитесь обработки. Пока материал не готов, он еще не доступен через MCP."
      ],
      "shot": "meetings.png",
      "details": [
        [
          "Импорт по ссылке",
          "Ссылка на страницу сайта отличается от прямой ссылки на медиа. На компьютере могут потребоваться компоненты загрузки из настроек."
        ],
        [
          "Если обработка не удалась",
          "Откройте меню записи и повторите расшифровку после проверки модели. Перед повтором убедитесь, что на устройстве есть исходный звук."
        ]
      ],
      "tip": "Не удаляйте исходный файл, пока не проверили результат. Отмена обработки и удаление записи — разные действия.",
      "phone": {
        "title": "Запись и импорт на Android",
        "lead": "Откройте «Встречи». Здесь находятся запись, загрузка файла и импорт по ссылке.",
        "shot": "android-meetings.png",
        "steps": [
          "Сначала выберите проект через строку-фильтр над списком или меню ☰.",
          "Нажмите микрофон для записи либо «Загрузить аудио или видео» для готового файла.",
          "Остановите запись и дождитесь статуса «Расшифровано»."
        ],
        "details": [
          [
            "Ссылки",
            "Телефон принимает прямую ссылку на медиафайл и Яндекс.Диск. Страницы YouTube, Rutube и VK импортируются на компьютере."
          ],
          [
            "Проект",
            "Импорт и новая запись сохраняются в выбранном проекте."
          ]
        ],
        "caption": "Кнопка загрузки расположена под микрофоном.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Проекты и поиск",
      "title": "Держите материалы одной задачи вместе",
      "lead": "Проект объединяет связанные встречи и становится границей доступа внешнего ИИ.",
      "desktop": "В левом меню: «+ Проект». Правая кнопка по проекту открывает действия, включая подключение и отключение доступа к ИИ.",
      "steps": [
        "Создайте проект и выберите его в меню. Новая запись или импорт попадет в выбранный проект.",
        "Перенесите уже существующие записи через действие «В проект». Для нескольких записей используйте режим выбора.",
        "Ищите по названию и тексту расшифровки. Внутри открытой записи используйте отдельный поиск по репликам."
      ],
      "shot": "projects.png",
      "details": [
        [
          "Одна запись, один проект",
          "Перенос меняет принадлежность записи. Он не создает независимую копию. Проверяйте новый проект, если управляете доступом через MCP."
        ],
        [
          "Отметка ИИ",
          "Фиолетовый цвет и значок «ИИ» на компьютере означают сохраненное разрешение. Они не показывают, запущен ли сейчас ChatGPT или Claude."
        ]
      ],
      "tip": "Не включайте общий доступ ко всем материалам ради одной записи: поместите нужные записи в отдельный проект.",
      "phone": {
        "title": "Проекты в меню телефона",
        "lead": "Откройте ☰. Фиолетовый проект со значком «ИИ» разрешен для работы через MCP.",
        "shot": "android-projects.png",
        "steps": [
          "Нажмите «+ Проект», чтобы создать папку для встреч.",
          "Нажмите название проекта, чтобы открыть его записи. Стрелка раскрывает список прямо в меню.",
          "Удерживайте название проекта: появятся доступ к ИИ, переименование и удаление."
        ],
        "details": [
          [
            "Цвет и значок",
            "Они показывают сохраненное разрешение. Это не индикатор работающего ChatGPT или Claude."
          ],
          [
            "Перенос записи",
            "Используйте меню открытой встречи или режим выбора → «В проект»."
          ]
        ],
        "caption": "Долгое нажатие на название открывает действия проекта.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Текст и экспорт",
      "title": "Проверьте реплики, голоса и готовый документ",
      "lead": "Автоматическая расшифровка — основа для проверки. Таймкоды связывают текст с исходным звуком.",
      "desktop": "В меню записи и карточках доступны экспорт и действия над расшифровкой. Для нескольких встреч можно использовать групповое выделение.",
      "steps": [
        "Откройте расшифровку. Найдите нужную фразу и нажмите время, чтобы проверить ее по звуку.",
        "Проверьте разделение говорящих, задайте имена и уточните спорные реплики. Автоматическая разметка может ошибаться.",
        "Выберите экспорт: TXT, Markdown, Word или PDF. При наличии звука можно сохранить WAV. Саммари можно экспортировать отдельно."
      ],
      "details": [
        [
          "Звук на другом устройстве",
          "Если синхронизация аудио выключена, текст может быть доступен без воспроизведения. Включение передачи звука требует места и трафика."
        ],
        [
          "Удаление",
          "Удаление синхронизированной встречи распространяется на другие устройства. Экспорт — отдельная копия, которую стоит сохранить перед удалением."
        ]
      ],
      "tip": "Имена, числа, названия и атрибуцию говорящих проверяйте по оригиналу перед публикацией или отправкой.",
      "phone": {
        "title": "Текст и экспорт с телефона",
        "lead": "Откройте запись во «Встречах». Текст и готовое саммари находятся в отдельных карточках.",
        "shot": "android-transcript.png",
        "steps": [
          "Нажмите «Открыть расшифровку» для полного текста и таймкодов.",
          "В карточке нажмите значок копирования, «Поделиться» или экспорт.",
          "Выберите TXT, Markdown, Word или PDF. Звук можно экспортировать при наличии аудиофайла."
        ],
        "details": [
          [
            "Воспроизведение",
            "Для прослушивания на телефоне нужна локальная аудиозапись. Если приехал только текст, включите передачу звука в синхронизации."
          ]
        ],
        "caption": "Значки копирования, отправки и экспорта — справа в заголовке карточки.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Саммари и разборы",
      "title": "Превратите расшифровку в рабочие итоги",
      "lead": "Локальная языковая модель на компьютере может подготовить саммари, название, задачи, решения, тезисы и другие разборы.",
      "desktop": "Загрузите языковую модель с подтверждением в настройках. В открытой записи выберите нужный вид результата; первая генерация может занять время.",
      "steps": [
        "Дождитесь полной расшифровки. На компьютере установите языковую модель.",
        "Откройте запись и создайте саммари или выберите подходящий разбор: встреча, вебинар, интервью и другие варианты.",
        "Сопоставьте результат с исходным текстом. Скопируйте или экспортируйте подходящий вариант."
      ],
      "details": [
        [
          "Вопросы и перевод",
          "На компьютере можно задавать вопросы по записи и переводить текст. Эти результаты не передаются на Android существующей синхронизацией."
        ],
        [
          "Результат внешнего ИИ",
          "MCP сейчас только читает материалы. Ответ ChatGPT или Claude не сохраняется автоматически в саммари Sol Flow."
        ]
      ],
      "tip": "Локальная модель не требует отправки текста провайдеру ИИ. Через MCP запрошенный текст получает внешний клиент — это другой режим работы.",
      "phone": {
        "title": "Как получить саммари на телефон",
        "lead": "Телефон показывает готовые итоги, которые создал компьютер.",
        "shot": "android-summary.png",
        "steps": [
          "Синхронизируйте запись с Mac или Windows.",
          "На компьютере создайте саммари либо включите автоматические итоги для записей с других устройств.",
          "Синхронизируйте оба устройства и откройте карточку «Саммари» на телефоне."
        ],
        "details": [
          [
            "Разборы и вопросы",
            "Разборы, переводы и история вопросов остаются на компьютере. Android их сейчас не получает."
          ]
        ],
        "caption": "«Показать целиком» раскрывает полный текст саммари.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Карты мыслей",
      "title": "Посмотрите на запись как на структуру тем",
      "lead": "Карта помогает увидеть основные идеи, связи и детали разговора.",
      "desktop": "Создание карты выполняется на Mac/Windows локальной языковой моделью.",
      "steps": [
        "На компьютере откройте готовую запись и создайте карту.",
        "Откройте карту, изучите ветки и исправьте формулировки или структуру при необходимости.",
        "Сохраните изменения. Для презентации или документа экспортируйте карту в PNG или SVG."
      ],
      "details": [
        [
          "Изменившийся источник",
          "Если расшифровка поменялась, прежняя карта может устареть. Перед повторной генерацией сохраните важную ручную работу отдельно."
        ]
      ],
      "tip": "Карта упрощает материал. Для точных формулировок и спорных выводов возвращайтесь к расшифровке.",
      "phone": {
        "title": "Карта записи на Android",
        "lead": "Готовая карта появляется в записи после синхронизации с компьютером.",
        "shot": "android-map.png",
        "steps": [
          "Создайте карту в Sol Flow на Mac или Windows.",
          "После синхронизации откройте карту в записи на телефоне.",
          "Изменяйте масштаб и ветви, сохраните правки или экспортируйте изображение."
        ],
        "details": [
          [
            "Создание",
            "На Android нет языковой модели для создания карты. Просмотр и правка готовой карты работают на телефоне."
          ]
        ],
        "caption": "Карта открывается отдельным экраном.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Синхронизация",
      "title": "Телефон записывает, компьютер продолжает работу",
      "lead": "Синхронизация переносит встречи и проекты через ваш Яндекс.Диск или Google Drive. На всех устройствах используйте один провайдер и один аккаунт.",
      "desktop": "В настройках компьютера можно включить создание саммари для записей с других устройств.",
      "steps": [
        "В «Настройках» выберите облако и завершите вход по показанным шагам. Повторите на втором устройстве с тем же аккаунтом.",
        "Синхронизируйте устройство-источник, затем второе устройство. Дождитесь завершения и проверьте название, проект и текст.",
        "На компьютере подготовьте саммари или карту и снова синхронизируйте оба устройства, чтобы увидеть результат на телефоне."
      ],
      "details": [
        [
          "Что синхронизируется",
          "Записи, расшифровки, проекты, названия, саммари и карты. Звук передается только при включенной настройке. История диктовок, вопросы, переводы, разборы не входят в этот маршрут."
        ],
        [
          "Если чего-то не видно",
          "Сверьте аккаунт, провайдер, состояние синхронизации и наличие готовой расшифровки на источнике. Приложению нужны интернет, свободное место и время."
        ],
        [
          "Доступ к ИИ",
          "Разрешения проектов синхронизируются между обновленными устройствами."
        ]
      ],
      "tip": "Разрешения ИИ теперь общие для синхронизируемых проектов. Для применения на другом устройстве нужны обновленный Sol Flow и завершенная синхронизация.",
      "phone": {
        "title": "Синхронизация телефона и компьютера",
        "lead": "Откройте ☰ → «Настройки» → «Синхронизация». На обоих устройствах выберите одно облако и один аккаунт.",
        "shot": "android-sync.png",
        "steps": [
          "Подключите Яндекс.Диск или Google Drive и завершите вход.",
          "Запустите «Синхронизировать» на устройстве с изменением, затем на другом.",
          "Проверьте запись, проект и итоговый текст. Для аудио отдельно включите передачу звука."
        ],
        "details": [
          [
            "Доступ к ИИ",
            "Разрешения проектов теперь тоже передаются через синхронизацию. Для применения нужны обновленные приложения и синхронизация обоих устройств."
          ],
          [
            "Если устройство выключено",
            "Оно получит изменение, когда Sol Flow снова запустится и синхронизируется. Отзыв с телефона не мгновенно закрывает источник на выключенном компьютере."
          ]
        ],
        "caption": "Настройки синхронизации находятся внутри общих настроек приложения.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Доступ через MCP",
      "title": "Откройте ИИ только выбранный проект",
      "lead": "MCP позволяет подключенному клиенту искать и читать разрешенные материалы Sol Flow с названием источника и таймкодами.",
      "desktop": "Это локальная функция Mac/Windows. Клиент запускает компонент Sol Flow на том же компьютере.",
      "steps": [
        "Откройте проект → «Доступ проекта к ИИ». Выберите нужные типы материалов: расшифровки, саммари, карты или разборы.",
        "Нажмите «Подключить проект». У проекта появятся фиолетовый цвет и значок «ИИ». Настройте клиент отдельно в следующем разделе.",
        "Для отзыва нажмите «Отключить доступ» или используйте меню проекта. Повторное подключение доступно здесь же."
      ],
      "shot": "mcp.png",
      "details": [
        [
          "Что получает клиент",
          "Только готовые выбранные тексты по запросу. Аудио и ключи в MCP-экспорт не входят. Разрешения действуют для клиентов, подключенных к этому экспорту, а не отдельно для каждого бренда ИИ."
        ],
        [
          "После отключения",
          "Новые запросы к проекту закрыты. Уже отправленный текст остается в истории внешнего чата."
        ],
        [
          "Текстовая копия",
          "Это отдельный экспорт всех разрешенных проектов. Он остается на диске после отзыва доступа; его изменение не редактирует оригиналы."
        ]
      ],
      "tip": "Разрешение синхронизируется с обновленными устройствами. Отзыв с другого устройства применяется после синхронизации; уже отправленные ответы остаются в истории.",
      "phone": {
        "title": "Включить или отключить проект для ИИ",
        "lead": "Откройте проект и нажмите «Подключить к ИИ» под его названием. Второй путь — удерживать проект в меню ☰ и выбрать «Доступ проекта к ИИ».",
        "shot": "android-ai-settings.png",
        "steps": [
          "Отметьте материалы: расшифровки, саммари, карты или разборы.",
          "Нажмите «Подключить к ИИ». Проект станет фиолетовым и получит значок «ИИ».",
          "Для отзыва нажмите «Отключить доступ к ИИ». Синхронизируйте телефон и компьютер."
        ],
        "details": [
          [
            "Где работает MCP",
            "Сервер остается на компьютере. Телефон передает разрешение, а компьютер предоставляет выбранные материалы подключенному клиенту. Сам клиент ChatGPT или Claude настраивается отдельно."
          ],
          [
            "Когда действует изменение",
            "На телефоне выбор сохраняется сразу. На другом устройстве — после синхронизации. Доступ по умолчанию выключен."
          ],
          [
            "Что передается",
            "Только выбранные готовые тексты. Аудио и ключи в MCP не входят. Ранее полученные ответы остаются в истории внешнего чата."
          ]
        ],
        "caption": "Галочки выбирают материалы, кнопка внизу сохраняет разрешение.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "ChatGPT и Claude",
      "title": "Подключите клиент и проверьте источники",
      "lead": "Разрешить проект и настроить клиент — два отдельных шага. Надпись «Настройка подключения к ИИ» предлагает инструкции, а не определяет активный клиент.",
      "desktop": "Откройте проект → «Доступ проекта к ИИ» → «Подключение клиента и дополнительные действия» → «Настройка подключения к ИИ».",
      "steps": [
        "Выберите ChatGPT / Codex или Claude Desktop и нажмите «Скопировать запрос для подключения». Отправьте запрос помощнику на том же компьютере: пути и инструкции уже включены.",
        "Помощнику нужен доступ к локальным файлам. Без него раскройте «Настроить вручную»: скопируйте команду и каждый аргумент в отдельные поля. Полная конфигурация предназначена только для файла настроек. Если потребуется, перезапустите подключение.",
        "В новом чате попросите перечислить проекты Sol Flow, открыть тестовую запись и назвать факт с таймкодом. Затем отключите доступ в Sol Flow и попросите заново прочитать источник."
      ],
      "details": [
        [
          "Проверочный запрос",
          "«Используй Sol Flow MCP. Перечисли доступные проекты и записи. Прочитай тестовую расшифровку и назови один факт с названием записи и таймкодом. Если источник недоступен, сообщи об этом»."
        ],
        [
          "Важно для Claude",
          "Локальные серверы проверяйте в обычном чате Claude Desktop. Настройка локального stdio не подключает сервер к claude.ai или Cowork."
        ],
        [
          "Если клиент не видит сервер",
          "Проверьте выбранную инструкцию, путь к установленному компоненту и каталог экспорта. Не заменяйте целиком существующий файл конфигурации. Ошибка пути после переноса приложения требует обновления параметров."
        ]
      ],
      "tip": "После отзыва ИИ может повторить уже прочитанный текст из истории. Проверяйте новый вызов инструмента, а не только содержание ответа.",
      "phone": {
        "title": "ChatGPT и Claude: что делать на телефоне",
        "lead": "Android управляет проектами и их разрешениями. Подключение локального клиента выполняется в Sol Flow на компьютере.",
        "shot": "android-project-menu.png",
        "steps": [
          "На телефоне включите проект для ИИ и синхронизируйте его с компьютером.",
          "На компьютере откройте проект → «Доступ проекта к ИИ» → «Настройка подключения к ИИ».",
          "Выберите инструкцию ChatGPT / Codex или Claude Desktop, затем проверьте чтение тестовой записи."
        ],
        "details": [
          [
            "Мобильный чат",
            "Включение проекта на телефоне само по себе не подключает мобильное приложение ChatGPT или Claude к локальному серверу."
          ],
          [
            "Проверка",
            "Попросите ИИ назвать источник и таймкод. После отключения доступа проверяйте новый вызов инструмента, а не повторение старого ответа."
          ]
        ],
        "caption": "Меню проекта открывается долгим нажатием на его название.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    },
    {
      "nav": "Настройки и помощь",
      "title": "Подстройте приложение и проверьте результат",
      "lead": "Настройки помогают выбрать удобный ввод, внешний вид и режим работы. Не все параметры одинаковы на телефоне и компьютере.",
      "desktop": "На компьютере доступны микрофон, горячая клавиша, параметры локальной модели и компоненты импорта ссылок.",
      "steps": [
        "В «Настройках» выберите язык, тему и подходящие параметры записи.",
        "В «Моделях» проверьте активную модель и свободное место. Удаление модели не заменяет удаление записей.",
        "В «О проекте» можно открыть это руководство, проверить обновление и подготовить отчет о проблеме. Отправка отчета остается вашим действием."
      ],
      "details": [
        [
          "Перед обновлением",
          "Устанавливайте сборку поверх существующего приложения. Если система предлагает удалить приложение из-за другой подписи, остановитесь: удаление может стереть данные."
        ],
        [
          "Перед релизом",
          "Проверяйте запись, импорт, экспорт, синхронизацию в обе стороны, карту и подключение MCP на реальных устройствах. Успешная сборка сама по себе этого не подтверждает."
        ]
      ],
      "tip": "Если возникла ошибка, сохраните ее текст, версию приложения и шаги воспроизведения. Не включайте пароли, коды входа и приватные расшифровки в публичный отчет.",
      "phone": {
        "title": "Настройки Android",
        "lead": "Откройте ☰ → «Настройки». Здесь меняются язык, тема, запись, плавающая кнопка и синхронизация.",
        "shot": "android-settings.png",
        "steps": [
          "Выберите удобный язык и тему.",
          "Проверьте настройки микрофона и плавающей кнопки. Для изогнутого экрана доступны увеличенные боковые отступы.",
          "В меню ☰ → «О проекте» можно снова открыть руководство или подготовить сообщение об ошибке."
        ],
        "details": [
          [
            "Обновление",
            "Устанавливайте новую сборку поверх существующей. Не удаляйте приложение ради обновления: это может стереть записи."
          ]
        ],
        "caption": "Все общие настройки доступны через меню ☰.",
        "tip": "Скриншот снят в приложении Android с учебными данными."
      }
    }
  ],
  "en": [
    {
      "nav": "Start",
      "title": "Turn speech into useful working material",
      "lead": "Sol Flow records and transcribes speech on your device. Organize meetings into projects, find passages, prepare results, and share selected text with an AI client.",
      "desktop": "Mac and Windows run speech recognition and a local language model. MCP is a separate connection to an AI client you choose.",
      "steps": [
        "Download a speech model for your language in Models. Downloading needs internet; transcription does not.",
        "Allow microphone access. Enable the additional permissions shown in setup if you want dictation inserted into other apps.",
        "Try a short dictation or import before processing a long meeting."
      ],
      "details": [
        [
          "Two kinds of AI",
          "Speech recognition turns audio into text. A desktop language model creates summaries and other results. External ChatGPT or Claude uses a separate MCP connection."
        ],
        [
          "What may leave the device",
          "Sync sends data to your selected cloud. MCP sends requested, permitted text to an AI client. You enable each separately."
        ]
      ],
      "tip": "Reopen this guide from About → Sol Flow guide. Use the device selector to see platform differences.",
      "phone": {
        "title": "Sol Flow on your phone",
        "lead": "Use the bottom tabs for Dictation, Meetings, History and Models. The ☰ menu contains projects, sync and settings.",
        "shot": "android-dictation.png",
        "steps": [
          "Download a speech model in Models.",
          "Allow microphone access; enable the floating button and required permissions for input into other apps.",
          "Use Meetings for long recordings."
        ],
        "details": [],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Dictation",
      "title": "Dictate into the app you are using",
      "lead": "Dictation is for messages, notes, and short passages. Use Meetings for longer conversations.",
      "mac": "The default Mac shortcut is Option + Space. Inserting text into other apps requires Accessibility permission.",
      "windows": "Check the assigned keyboard shortcut in Settings. The destination text field must be focused.",
      "steps": [
        "Place the cursor in the destination field.",
        "Tap the button or shortcut to start; tap again to stop. Holding records until you release.",
        "Wait for recognition and check the inserted text. Previous dictations are available in History."
      ],
      "shot": "dictation.png",
      "details": [
        [
          "If insertion fails",
          "Check permissions and input focus. Find the result in History and copy it manually."
        ],
        [
          "Language and model",
          "Select a suitable speech model in Models or through the active model selector."
        ]
      ],
      "tip": "Check your microphone and language before speaking. Switching models may take a moment.",
      "phone": {
        "title": "Dictation and the floating button",
        "lead": "The first screen shows your model, microphone and permission status.",
        "shot": "android-dictation.png",
        "steps": [
          "Tap the microphone, speak, then tap again to stop.",
          "For a message, focus its input field and use the Sol Flow floating button.",
          "Find the result in History if insertion fails."
        ],
        "details": [],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Meetings and import",
      "title": "Record a conversation or import a file",
      "lead": "Sol Flow saves meeting audio and produces a timestamped transcript. Recording can be paused and resumed.",
      "desktop": "Mac/Windows support audio/video files, direct media links, Yandex Disk, and supported YouTube, Rutube, and VK pages.",
      "steps": [
        "Open Meetings and choose a project before recording or importing.",
        "Record a meeting or select a file. On desktop you can drag a file into the window; links have a separate input.",
        "Stop and wait for processing. Unfinished material is not available through MCP."
      ],
      "shot": "meetings.png",
      "details": [
        [
          "Page versus media link",
          "A website page is different from a direct media URL. Desktop downloading may require components installed from Settings."
        ],
        [
          "Retrying",
          "Use the recording menu to transcribe again after checking the model and availability of the original audio."
        ]
      ],
      "tip": "Keep the original until you have checked the result. Cancelling processing and deleting a recording are separate actions.",
      "phone": {
        "title": "Recording and importing on Android",
        "lead": "Open Meetings for recording, file import and links.",
        "shot": "android-meetings.png",
        "steps": [
          "Choose a project in the filter above the list or the ☰ menu.",
          "Tap the microphone or Load audio or video.",
          "Stop and wait until the recording is transcribed."
        ],
        "details": [
          [
            "Links",
            "Android imports direct media links and Yandex Disk. Import YouTube, Rutube and VK pages on desktop."
          ]
        ],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Projects and search",
      "title": "Keep related material together",
      "lead": "A project groups related meetings and defines a boundary for external AI access.",
      "desktop": "Use + Project in the sidebar. Right-click a project for actions, including AI access controls.",
      "steps": [
        "Create and select a project. New recordings and imports go into the selected project.",
        "Move existing recordings with To project. Use selection mode for multiple recordings.",
        "Search recording titles and transcript text. An open recording has its own passage search."
      ],
      "shot": "projects.png",
      "details": [
        [
          "One recording, one project",
          "Moving a recording changes its project; it does not create an independent copy. Check the destination when managing MCP access."
        ],
        [
          "AI marker",
          "The purple desktop marker indicates saved permission. It does not show whether ChatGPT or Claude is currently running."
        ]
      ],
      "tip": "Use a separate project for the recordings you want to share instead of granting access to unrelated material.",
      "phone": {
        "title": "Projects in the phone menu",
        "lead": "Open ☰. Purple projects with an AI icon have saved MCP permission.",
        "shot": "android-projects.png",
        "steps": [
          "Use + Project to create a folder.",
          "Tap its name to open recordings; the arrow expands them in the drawer.",
          "Long-press its name for AI access, rename or delete."
        ],
        "details": [],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Review and export",
      "title": "Check passages, speakers, and your document",
      "lead": "An automatic transcript needs review. Timestamps connect text to the original sound.",
      "desktop": "Use the recording menu and result cards to export. Selection mode supports multiple meetings.",
      "steps": [
        "Find the passage and tap its timestamp to check it against the audio.",
        "Review speaker separation, assign names, and clarify uncertain passages. Automatic attribution may be wrong.",
        "Export TXT, Markdown, Word, or PDF. Available audio can be exported as WAV; summaries can be exported separately."
      ],
      "details": [
        [
          "Audio on another device",
          "Text can be available without playback when audio sync is off. Transferring sound uses storage and data."
        ],
        [
          "Deleting",
          "Deleting a synced meeting propagates to other devices. An export is an independent copy to save before deletion."
        ]
      ],
      "tip": "Verify names, numbers, terminology, and speaker attribution before sharing or publishing.",
      "phone": {
        "title": "Review and export on your phone",
        "lead": "Open a recording. Transcript and summary have separate cards.",
        "shot": "android-transcript.png",
        "steps": [
          "Tap Open the transcript for full text and timestamps.",
          "Use the copy, Share or export icon in the card header.",
          "Choose TXT, Markdown, Word or PDF. Audio export requires a local recording."
        ],
        "details": [],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Summaries and analysis",
      "title": "Turn a transcript into working conclusions",
      "lead": "The desktop language model creates summaries, titles, tasks, decisions, outlines, and other analyses.",
      "desktop": "Download the language model with your confirmation in Settings. Then select the result you need in an open recording.",
      "steps": [
        "Wait for the transcript and install the desktop language model.",
        "Create a summary or select a suitable analysis for a meeting, webinar, interview, or another recording type.",
        "Compare the result with its source, then copy or export it."
      ],
      "details": [
        [
          "Questions and translation",
          "Desktop can answer questions about a recording and translate text. These results are not sent to Android by the current sync."
        ],
        [
          "External AI results",
          "MCP is read-only. ChatGPT or Claude answers are not automatically saved into Sol Flow summaries."
        ]
      ],
      "tip": "The local model does not send text to an AI provider. MCP is a separate mode in which an external client receives requested text.",
      "phone": {
        "title": "Receive summaries on Android",
        "lead": "Your phone displays results created by a computer.",
        "shot": "android-summary.png",
        "steps": [
          "Sync the recording to Mac or Windows.",
          "Generate a summary there or enable automatic summaries for other devices.",
          "Sync both devices and open the Summary card on the phone."
        ],
        "details": [
          [
            "Desktop results",
            "Android receives summaries and titles. Analyses, translations and question history currently stay on desktop."
          ]
        ],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Mind maps",
      "title": "Explore the structure of a conversation",
      "lead": "A map shows main ideas, connections, and details.",
      "desktop": "Mac/Windows generate maps with the local language model.",
      "steps": [
        "Create a map from a finished recording on desktop.",
        "Explore branches and edit wording or structure as needed.",
        "Save changes or export PNG/SVG for a document or presentation."
      ],
      "details": [
        [
          "Changed source",
          "A map may become stale after the transcript changes. Save important manual work separately before regenerating."
        ]
      ],
      "tip": "Maps simplify information. Return to the transcript for exact wording and uncertain conclusions.",
      "phone": {
        "title": "Mind maps on Android",
        "lead": "A finished map arrives from the computer through sync.",
        "shot": "android-map.png",
        "steps": [
          "Create a map on Mac or Windows.",
          "Sync and open the recording’s map on your phone.",
          "Explore or edit branches; save changes or export an image."
        ],
        "details": [
          [
            "Generation",
            "Android has no language model for map generation. It can view and edit a finished map."
          ]
        ],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Sync",
      "title": "Record on your phone, continue on your computer",
      "lead": "Sync transfers meetings and projects through your Yandex Disk or Google Drive. Use the same provider and account on every device.",
      "desktop": "Desktop Settings can enable automatic summaries for recordings from other devices.",
      "steps": [
        "Choose a cloud in Settings and complete sign-in. Repeat on the other device using the same account.",
        "Sync the source device, then the other device. Wait for completion and check title, project, and text.",
        "Create a summary or map on desktop and sync both devices again to receive it on the phone."
      ],
      "details": [
        [
          "Included material",
          "Meetings, transcripts, projects, titles, summaries, and maps. Audio is optional. Dictation history, questions, translations, and analyses are not part of this route."
        ],
        [
          "Missing items",
          "Check provider, account, sync status, and whether the source transcript is ready. Internet, free space, and time are required."
        ],
        [
          "AI access",
          "Project permissions sync between updated devices."
        ]
      ],
      "tip": "AI permissions are now shared for synced projects. Other devices need updated Sol Flow and a completed sync.",
      "phone": {
        "title": "Sync your phone and computer",
        "lead": "Open ☰ → Settings → Sync. Use the same cloud provider and account on both devices.",
        "shot": "android-sync.png",
        "steps": [
          "Connect Yandex Disk or Google Drive.",
          "Sync the device containing the change, then the other device.",
          "Check the project and text. Enable audio transfer separately if needed."
        ],
        "details": [
          [
            "AI permissions",
            "Project permissions now sync too. Both devices need updated apps. Changes reach an offline computer after Sol Flow starts and syncs."
          ]
        ],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "MCP access",
      "title": "Give AI access to a selected project",
      "lead": "MCP lets a connected client search and read permitted Sol Flow material with source names and timestamps.",
      "desktop": "This is a local Mac/Windows feature. The client starts the Sol Flow component on the same computer.",
      "steps": [
        "Open a project → Project access for AI. Select transcripts, summaries, maps, or analyses.",
        "Click Connect project. A purple AI marker appears. Configure your AI client separately in the next section.",
        "Revoke access with the button or project context menu. Reconnect from the same controls."
      ],
      "shot": "mcp.png",
      "details": [
        [
          "What the client receives",
          "Only selected, finished text on request. Audio and credentials are excluded. Grants apply to clients using this export, not separately to each AI brand."
        ],
        [
          "After revocation",
          "New requests are blocked. Previously transmitted text remains in external chat history."
        ],
        [
          "Text copy",
          "This exports all permitted projects separately. The copy remains after revocation; editing it does not edit the originals."
        ]
      ],
      "tip": "Permissions sync to updated devices. Remote revocation applies after sync; previous chat responses remain.",
      "phone": {
        "title": "Connect or disconnect a project for AI",
        "lead": "Open a project and tap Connect to AI below its name. Or long-press the project in ☰ and choose Project access for AI.",
        "shot": "android-ai-settings.png",
        "steps": [
          "Choose transcripts, summaries, maps or analyses.",
          "Tap Connect to AI. The project becomes purple with an AI icon.",
          "To revoke, tap Disconnect AI access and sync both devices."
        ],
        "details": [
          [
            "Where MCP runs",
            "On a computer. The phone syncs permission; the computer serves selected text to a separately configured AI client."
          ],
          [
            "When it takes effect",
            "Permission is saved immediately on the phone and applied on another device after both devices sync. An offline computer may still serve its previous export until it syncs."
          ],
          [
            "What is shared",
            "Selected finished text only. No audio or keys. Prior chat responses remain after revocation."
          ]
        ],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "ChatGPT and Claude",
      "title": "Connect a client and verify its sources",
      "lead": "Project permission and client setup are separate steps. AI connection setup provides instructions; it does not detect the active client.",
      "desktop": "Open a project → Project access for AI → Client setup and more options → AI connection setup.",
      "steps": [
        "Choose ChatGPT / Codex or Claude Desktop and click Copy setup request. Send it to an assistant on the same computer: paths and instructions are already included.",
        "The assistant needs local file access. Without it, expand Set up manually: copy the command and each argument into separate fields. The full configuration belongs only in the settings file. Restart the connection if needed.",
        "In a new chat, request the Sol Flow projects, read a test recording, and ask for a fact with a timestamp. Revoke access in Sol Flow and request a fresh source read."
      ],
      "details": [
        [
          "Test prompt",
          "“Use Sol Flow MCP. List available projects and recordings. Read a test transcript and give one fact with the recording title and timestamp. If the source is unavailable, say so.”"
        ],
        [
          "Claude scope",
          "Test local servers in a normal Claude Desktop chat. Local stdio configuration does not connect the server to claude.ai or Cowork."
        ],
        [
          "If the server is missing",
          "Check the selected instructions, installed server path, and export directory. Do not replace the entire existing config. Moving the app may require updated paths."
        ]
      ],
      "tip": "After revocation, AI can repeat cached text. Check a fresh tool call, not just the wording of the answer.",
      "phone": {
        "title": "ChatGPT and Claude: the phone’s role",
        "lead": "Android manages project permissions. A local AI client connects on the computer.",
        "shot": "android-project-menu.png",
        "steps": [
          "Enable the project on the phone and sync it to your computer.",
          "On desktop open the project → Project access for AI → AI connection setup.",
          "Choose ChatGPT / Codex or Claude Desktop instructions and test a recording."
        ],
        "details": [
          [
            "Mobile chats",
            "Enabling a project on Android does not automatically connect mobile ChatGPT or Claude to your computer’s local MCP server."
          ]
        ],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    },
    {
      "nav": "Settings and help",
      "title": "Adapt the app and check your results",
      "lead": "Settings control input, appearance, and processing. Available options differ by platform.",
      "desktop": "Desktop includes microphone, keyboard shortcut, local model, and link import component settings.",
      "steps": [
        "Choose language, theme, and recording preferences in Settings.",
        "Check the active model and free space in Models. Removing a model is different from deleting recordings.",
        "Use About to reopen this guide, check updates, or prepare an issue report. You decide whether to send it."
      ],
      "details": [
        [
          "Updating",
          "Install over the existing app. If a signature mismatch requires uninstalling, stop: uninstalling may erase data."
        ],
        [
          "Before release",
          "Check recording, import, export, bidirectional sync, maps, and MCP on real devices. A successful build alone does not confirm these."
        ]
      ],
      "tip": "Keep the error message, app version, and reproduction steps. Exclude passwords, login codes, and private transcripts from public reports.",
      "phone": {
        "title": "Android settings",
        "lead": "Open ☰ → Settings for language, appearance, recording, floating button and sync.",
        "shot": "android-settings.png",
        "steps": [
          "Choose your language and theme.",
          "Check microphone and floating button preferences; increased side margins support curved screens.",
          "Use ☰ → About to reopen this guide or prepare an issue report."
        ],
        "details": [],
        "caption": "Screenshot uses Russian labels.",
        "tip": "The screenshot shows the Android app with sample data."
      }
    }
  ]
};
