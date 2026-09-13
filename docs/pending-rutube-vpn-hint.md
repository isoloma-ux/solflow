> Implemented and installed locally as 0.9.9 on 2026-09-13; not publicly released. Details below record the earlier preparation stage. See [0.9.9](release-0.9.9.md).

# Rutube connection hint — pending release / следующий выпуск

## English

Mac and Windows show a contextual VPN/proxy hint when a Rutube download fails because of a connection/DNS error or HTTP 403. The hint suggests disabling VPN/proxy if enabled; it does not claim that VPN is the cause. Retry submits the same link without browser-session access. Changing the input hides the hint; an older failure cannot overwrite another link. Deleted/private videos, unsupported URLs, rate limits and cancellations retain their existing messages.

Android does not import Rutube pages, so this desktop import hint does not apply there. No VPN detection, network settings changes or release publication are included. Prepared locally for a future release; installed/published 0.9.8 is unchanged.

## Русский

Mac и Windows показывают подсказку о VPN/прокси при ошибке подключения/DNS или HTTP 403 во время загрузки Rutube. Подсказка предлагает отключить VPN/прокси, если он включён, но не объявляет его причиной ошибки. «Повторить» отправляет ту же ссылку без доступа к сессии браузера. Изменение ссылки скрывает плашку; поздняя ошибка не подменяет другую введённую ссылку. Для удалённого/закрытого видео, неподдерживаемой ссылки, ограничения частоты запросов и отмены сохраняются прежние сообщения.

Android не импортирует страницы Rutube, поэтому эта подсказка относится к настольному импорту. Определение VPN, изменение сетевых настроек и публикация релиза не выполняются. Изменения подготовлены локально для следующего выпуска; установленная и опубликованная 0.9.8 остаётся прежней.

## Validation / Проверки

- 12 Rust downloader regression checks passed, including Rutube DNS/403 classification and exclusions.
- Isolated production UI checks passed in Russian/dark/1440, English/light/920 and Russian/light/700: translated hint, same-link retry, no browser session and stale-error protection.
- RU/EN hint screenshots visually reviewed. Full native release builds are deferred until the next release.
