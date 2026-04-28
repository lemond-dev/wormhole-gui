<script>
  import Icon from '../Icon.svelte';

  export let placeholder = '输入消息或拖入文件…';
  export let onSend; // (text) => Promise<void>

  let value = '';
  let sending = false;

  async function doSend() {
    const trimmed = value.trim();
    if (!trimmed || sending) return;
    sending = true;
    try {
      await onSend(trimmed);
      value = '';
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
    <button class="icon-btn attach" title="附件 (Ctrl+O)" disabled>
      <Icon name="paperclip" size={16} />
    </button>
    <textarea
      bind:value
      placeholder={placeholder}
      rows={1}
      on:keydown={handleKey}
      style:height={value ? '44px' : '22px'}
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
