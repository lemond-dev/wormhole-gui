<script>
  import { tick } from 'svelte';
  import Icon from '../Icon.svelte';

  export let placeholder = '输入消息或拖入文件…';
  export let onSend; // (text) => Promise<void>
  export let onAttach = null; // () => Promise<void>

  let value = '';
  let sending = false;
  let textareaEl;

  // Reset height to 'auto' so it can also shrink when text is deleted, then
  // grow up to the CSS max-height. Reading bind:this refs inside `$:` is the
  // safe_not_equal infinite-loop trap (see Session.svelte), so this is only
  // ever called from on:input or explicitly after we clear `value`.
  function autoresize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 120) + 'px';
  }

  async function doSend() {
    const trimmed = value.trim();
    if (!trimmed || sending) return;
    sending = true;
    try {
      await onSend(trimmed);
      value = '';
      await tick();
      autoresize();
    } finally {
      sending = false;
    }
  }

  function handleKey(e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      doSend();
    }
  }
</script>

<div class="wm-composer">
  <div class="row">
    <button
      class="icon-btn attach"
      title="附件 (Ctrl+O)"
      disabled={!onAttach}
      on:click={() => onAttach && onAttach()}
    >
      <Icon name="paperclip" size={16} />
    </button>
    <textarea
      bind:value
      bind:this={textareaEl}
      placeholder={placeholder}
      rows={1}
      on:keydown={handleKey}
      on:input={autoresize}
    ></textarea>
    <button
      class="send-btn"
      disabled={!value.trim() || sending}
      title="发送 (Enter)"
      on:click={doSend}
    >
      <Icon name="send" size={14} stroke={2.2} />
    </button>
  </div>
</div>
