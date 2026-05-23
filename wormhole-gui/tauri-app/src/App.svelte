<script>
  import { onMount, onDestroy } from 'svelte';
  import './styles.css';

  import { appState } from './lib/store.js';
  import { setupListeners, teardownListeners, triggerUpdateCheck, getConfig } from './lib/ipc.js';
  import { setLanguageFromConfig } from './lib/i18n';

  import Idle from './lib/screens/Idle.svelte';
  import Allocator from './lib/screens/Allocator.svelte';
  import Joiner from './lib/screens/Joiner.svelte';
  import Connecting from './lib/screens/Connecting.svelte';
  import Session from './lib/screens/Session.svelte';
  import ErrorScreen from './lib/screens/Error.svelte';
  import Closed from './lib/screens/Closed.svelte';
  import Settings from './lib/screens/Settings.svelte';
  import UpdateBanner from './lib/components/UpdateBanner.svelte';

  // 2-second delay so the initial paint completes before we hit the
  // network — keeps cold-start feel snappy and avoids a network blip on
  // the splash screen.
  let updateCheckTimer = null;
  onMount(async () => {
    setupListeners();
    // Pull persisted language out of Config and tell svelte-i18n. Boot
    // already defaults to `zh`, so this is a no-op for fresh installs;
    // a returning user who switched to English sees the change after one
    // frame of Chinese rather than the full first paint.
    try {
      const cfg = await getConfig();
      setLanguageFromConfig(cfg.language);
    } catch (e) {
      console.warn('config load failed; staying on default locale', e);
    }
    updateCheckTimer = setTimeout(() => {
      triggerUpdateCheck({ silent: true });
    }, 2000);
  });
  onDestroy(() => {
    if (updateCheckTimer) clearTimeout(updateCheckTimer);
    teardownListeners();
  });
</script>

<UpdateBanner />

{#if $appState === 'idle'}
  <Idle />
{:else if $appState === 'allocator-wait'}
  <Allocator />
{:else if $appState === 'joiner-input'}
  <Joiner />
{:else if $appState === 'connecting'}
  <Connecting />
{:else if $appState === 'connected'}
  <Session />
{:else if $appState === 'error'}
  <ErrorScreen />
{:else if $appState === 'closed'}
  <Closed />
{:else if $appState === 'settings'}
  <Settings />
{/if}
