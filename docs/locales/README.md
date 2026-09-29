# Offline UI translations

Russian and English retain their existing platform source files. `source.json` lists the English UI keys shared by the six additional languages. Each locale JSON maps the exact English text to its translation. Chinese uses Simplified Chinese.

When adding a UI string, add its English source to `source.json` and all six dictionaries. Keep placeholders (`{0}`, `%1$s`), commands, paths, URLs and product names intact. Release-note bullet lists are composed from individual translated sentences.

Generate and check bundled resources from the repository root:

```sh
python3 scripts/build-ui-locales.py
python3 scripts/build-ui-locales.py --check
node scripts/check-language-search.cjs
node scripts/check-ui-locales.cjs
```

The generator writes Android `values-*` resources and the shared desktop/Android guide dictionary. Android catalog descriptions, desktop messages and the guide use the same offline dictionaries. The optional `--ios-root` argument exports a resource for the separate iPhone test workspace.

Only application interface strings go through these dictionaries. Recordings, project names, transcripts and generated summaries are never translated by this mechanism. The interface setting does not select a speech-recognition language or modify sync data.

Translations were prepared and reviewed by AI translation agents; native-speaker editorial review has not been completed. Runtime checks cover key completeness, placeholders, locale selection and search behavior; they do not establish linguistic perfection.
