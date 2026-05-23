// i18n must be imported before App.svelte so `init()` runs before any
// component touches `$_`. Side-effect import — its `init()` call at
// module evaluation time is what wires up the dictionaries.
import './lib/i18n';
import App from './App.svelte';

const app = new App({
  target: document.getElementById('app'),
});

export default app;
