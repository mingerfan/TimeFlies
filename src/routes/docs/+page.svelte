<main class="docs-screen">
  <header class="page-head">
    <div>
      <p class="eyebrow">文档页面</p>
      <h1>命令速查</h1>
      <p class="sub">临时独立页面，后续可迁移到设置模块。</p>
    </div>
  </header>

  <section class="panel">
    <div class="panel-head">
      <h2>核心命令</h2>
      <p>默认作用于“当前操控目标”任务</p>
    </div>
    <div class="guide-grid">
      <article class="guide-card">
        <p class="guide-cmd">/rename 新标题</p>
        <p class="guide-desc">重命名当前任务。</p>
      </article>
      <article class="guide-card">
        <p class="guide-cmd">/parent root</p>
        <p class="guide-desc">将当前任务提升为根任务。</p>
      </article>
      <article class="guide-card">
        <p class="guide-cmd">/parent 任务ID</p>
        <p class="guide-desc">将当前任务挂到指定父任务下。</p>
      </article>
      <article class="guide-card">
        <p class="guide-cmd">/start | /pause | /resume | /stop</p>
        <p class="guide-desc">控制当前任务状态机。</p>
      </article>
      <article class="guide-card">
        <p class="guide-cmd">/sub 子任务标题</p>
        <p class="guide-desc">在当前任务下创建子任务；运行中会插入并开始。</p>
      </article>
      <article class="guide-card">
        <p class="guide-cmd">写周报 #work #writing</p>
        <p class="guide-desc">纯文本创建任务并追加标签。</p>
      </article>
    </div>
  </section>

  <section class="panel notes-panel">
    <h2>操作说明</h2>
    <ul class="notes">
      <li>任务详情页顶部主工作台可一键切换操控目标到当前活动任务。</li>
      <li>若已有运行中任务，执行 /start 或 /resume 会先自动暂停原任务。</li>
      <li>命令执行失败不影响输入框，可直接修正命令后再次回车。</li>
    </ul>
  </section>
</main>

<style>
  .docs-screen {
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .page-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-shrink: 0;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.74rem;
    color: #3f608d;
  }

  h1 {
    margin: 0.15rem 0;
    font-size: clamp(1.6rem, 2.1vw, 2.05rem);
  }

  .sub {
    margin: 0;
    color: #3f5f85;
    font-size: 0.9rem;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 0.95rem;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .panel-head h2 {
    margin: 0;
    font-size: 1rem;
    color: #163a65;
  }

  .panel-head p {
    margin: 0;
    color: #4e6f95;
    font-size: 0.84rem;
  }

  .guide-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem;
  }

  .guide-card {
    border: 1px solid #cad9ee;
    border-radius: 0.7rem;
    background: #f7fbff;
    padding: 0.56rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    min-width: 0;
  }

  .guide-cmd {
    margin: 0;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: 0.79rem;
    color: #1d436d;
    line-height: 1.32;
    word-break: break-word;
  }

  .guide-desc {
    margin: 0;
    color: #52749a;
    font-size: 0.79rem;
    line-height: 1.35;
  }

  .notes-panel h2 {
    margin: 0;
    font-size: 1rem;
    color: #163a65;
  }

  .notes {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.38rem;
    color: #48698f;
    font-size: 0.86rem;
    line-height: 1.4;
  }

  @media (max-height: 700px) {
    .docs-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
    }

    .panel {
      overflow: visible;
    }
  }

  @media (max-width: 860px) {
    .guide-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
