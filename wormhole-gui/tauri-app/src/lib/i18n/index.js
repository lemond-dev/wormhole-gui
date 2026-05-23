// svelte-i18n boot. Two locales: zh (default) and en.
//
// Translations are loaded synchronously via `import` rather than the
// `register()` lazy-loader because the app is tiny (<5KB of strings) and
// shipping a single chunk is simpler than splitting on first render.
// Switching locale is `locale.set('en' | 'zh')` and is reactive — every
// `$_(...)` call site re-renders without restart.

import { addMessages, init, locale, getLocaleFromNavigator } from 'svelte-i18n';
import zh from './zh.json';
import en from './en.json';

addMessages('zh', zh);
addMessages('en', en);

// `init()` must run before any component uses `$_`, so we call it at
// module evaluation time. The initial locale is `zh` so users who launch
// the app before Config has loaded see Chinese (the historical default
// matches what every pre-0.3.1 install shipped).
init({
  fallbackLocale: 'zh',
  initialLocale: 'zh',
});

/**
 * Adopt a language from a persisted Config. Tolerates unknown values by
 * falling back to `zh`, matching the backend's ConfigState::language()
 * behaviour, so a typo in config.json doesn't leave the UI un-translated.
 */
export function setLanguageFromConfig(lang) {
  if (lang === 'zh' || lang === 'en') {
    locale.set(lang);
  } else {
    locale.set('zh');
  }
}

export { locale, getLocaleFromNavigator };
