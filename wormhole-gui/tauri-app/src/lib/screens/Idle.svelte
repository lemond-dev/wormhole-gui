<script>
  import Icon from '../Icon.svelte';
  import { startSend } from '../ipc.js';
  import { appState } from '../store.js';

  function chooseSend() { startSend(); }
  function chooseRecv() { appState.set('joiner-input'); }
</script>

<div class="wm-app">
  <div class="wm-idle">
    <div class="wordmark">
      <span class="badge"><Icon name="shield-check" size={13} stroke={2.2} /></span>
      Wormhole
    </div>
    <h1 style="margin-top: 28px;">建立一次性加密会话</h1>
    <div class="lede">
      用一个短码把两台设备配对，双向收发文字与文件。<br />
      关闭即销毁，没有账号、没有历史。
    </div>
    <div class="choices">
      <button class="wm-bigchoice" on:click={chooseSend}>
        <span class="icon"><Icon name="send" size={18} stroke={2} /></span>
        <span class="title">发送</span>
        <span class="sub">生成短码</span>
      </button>
      <button class="wm-bigchoice" on:click={chooseRecv}>
        <span class="icon"><Icon name="download" size={18} stroke={2} /></span>
        <span class="title">接收</span>
        <span class="sub">输入短码</span>
      </button>
    </div>
    <div class="footer-note">
      <Icon name="lock" size={11} />
      端到端加密的临时会话，不需要注册
    </div>
  </div>
</div>
