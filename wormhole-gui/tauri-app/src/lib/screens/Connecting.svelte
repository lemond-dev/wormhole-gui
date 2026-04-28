<script>
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { closeSession } from '../ipc.js';
  import { reset } from '../store.js';

  async function cancel() {
    await closeSession();
    reset();
  }
</script>

<div class="wm-app">
  <SimpleHeader title="连接中" onBack={cancel} />
  <div
    class="wm-flowpage"
    style="align-items:center; justify-content:center; text-align:center; gap:18px; padding:40px 32px;"
  >
    <div class="spinner-wrap">
      <div class="spinner"></div>
      <div class="spinner-icon">
        <Icon name="lock" size={20} stroke={2} />
      </div>
    </div>
    <div style="font-size:15px; font-weight:500;">正在协商密钥…</div>
    <div class="desc" style="text-align:center; max-width:280px;">
      短码已匹配，正在与对方建立加密通道。这一步通常只需要几秒。
    </div>
    <div class="wm-mt-auto"></div>
    <button class="wm-btn ghost" on:click={cancel}>取消</button>
  </div>
</div>

<style>
  .spinner-wrap {
    width: 56px;
    height: 56px;
    position: relative;
    margin: 0 auto;
  }
  .spinner {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 3px solid var(--surface-3);
    border-top-color: var(--brand);
    animation: wm-spin 0.9s linear infinite;
  }
  .spinner-icon {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--brand);
  }
</style>
