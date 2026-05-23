<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  export let code = '';
  export let onCopy;

  $: parts = code ? code.split('-') : ['', '', ''];
  $: nameplate = parts[0] || '';
  $: rest = parts.slice(1).join('-');
</script>

<div class="wm-codebanner">
  <div class="label">{$_('codeBanner.label')}</div>
  <div class="code">
    <span class="num">{nameplate}</span>{rest ? `-${rest}` : ''}
  </div>
  <div class="actions">
    <button class="wm-btn" on:click={onCopy} disabled={!code}>
      <Icon name="copy" size={14} /> {$_('common.copy')}
    </button>
  </div>
</div>
