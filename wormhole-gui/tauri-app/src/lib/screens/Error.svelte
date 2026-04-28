<script>
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { lastError, closeReason, reset } from '../store.js';

  // Map error/close reason to a kind for the design's error variants.
  $: kind = pickKind($lastError, $closeReason);

  function pickKind(err, reason) {
    const txt = `${err?.message || ''} ${reason || ''}`.toLowerCase();
    if (txt.includes('pake')) return 'pake';
    if (txt.includes('expired') || txt.includes('过期')) return 'expired';
    if (txt.includes('peer') || txt.includes('对方') || txt.includes('lonely')) return 'peer-closed';
    if (txt.includes('network') || txt.includes('rendezvous') || txt.includes('socket') || txt.includes('connection')) return 'network';
    return 'peer-closed';
  }

  $: meta = {
    pake: {
      icon: 'shield-alert',
      title: '短码不正确',
      body: '请检查后重新输入，或让对方重新生成。注意短码大小写敏感且不能复用。',
    },
    network: {
      icon: 'wifi-off',
      title: '连接已中断',
      body: '本次会话结束。请重新生成短码再试一次。',
    },
    'peer-closed': {
      icon: 'log-out',
      title: '对方已结束会话',
      body: '所有传输已停止。',
    },
    expired: {
      icon: 'alert-circle',
      title: '短码已过期',
      body: '短码 5 分钟未使用已失效，请重新生成。',
    },
  }[kind];

  function back() { reset(); }
  function restart() { reset(); }
</script>

<div class="wm-app">
  <SimpleHeader title="" onBack={back} />
  <div
    class="wm-flowpage"
    style="align-items:center; justify-content:center; text-align:center; padding:40px 32px;"
  >
    <div class="badge">
      <Icon name={meta.icon} size={26} stroke={1.6} />
    </div>
    <h2 style="margin-top:8px; text-align:center; font-size:18px;">{meta.title}</h2>
    <div class="desc" style="text-align:center; max-width:320px;">{meta.body}</div>
    {#if $lastError?.message}
      <div class="raw mono">{$lastError.message}</div>
    {/if}
    <div class="wm-row" style="justify-content:center; margin-top:8px;">
      <button class="wm-btn" on:click={back}>返回</button>
      <button class="wm-btn primary" on:click={restart}>
        <Icon name="refresh" size={13} /> 重新开始
      </button>
    </div>
  </div>
</div>

<style>
  .badge {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--text-3);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto;
  }
  .raw {
    margin-top: 12px;
    font-size: 11.5px;
    color: var(--text-4);
    max-width: 340px;
    word-break: break-all;
  }
</style>
