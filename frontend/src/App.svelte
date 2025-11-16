<script>
  import { onMount } from 'svelte';
  import { api, CATEGORIES, ALL_BOARDS } from './lib/api';

  let authMode = 'login';
  let loginForm = { username: '', password: '' };
  let registerForm = { username: '', qq: '', password: '' };
  let authError = '';
  let authSuccess = '';

  let currentUser = null;
  let activeSection = 'posts';
  let posts = [];
  let selectedCategory = '最新';
  let postsError = '';
  let loadingPosts = false;

  let composer = {
    title: '',
    content: '',
    category: '其它',
    anonymous: false
  };
  let composerMessage = '';
  let composerError = '';
  let posting = false;

  let postDetail = null;
  let detailError = '';
  let commentForm = { content: '', anonymous: false };
  let commentMessage = '';
  let commenting = false;

  let profileForm = { username: '', qq: '' };
  let profileMessage = '';
  let profileError = '';
  let passwordForm = { current_password: '', new_password: '', confirm: '' };
  let passwordMessage = '';
  let passwordError = '';

  let myPosts = [];
  let myPostsError = '';

  let publicProfile = null;
  let publicProfileError = '';

  onMount(() => {
    tryRestoreSession();
  });

  async function tryRestoreSession() {
    try {
      const me = await api.me();
      handleLoginSuccess(me);
    } catch (err) {
      console.debug('no active session', err?.message);
    }
  }

  async function handleLogin(event) {
    event?.preventDefault();
    authError = '';
    authSuccess = '';
    try {
      const user = await api.login(loginForm);
      loginForm = { username: '', password: '' };
      handleLoginSuccess(user);
    } catch (err) {
      authError = err.message;
    }
  }

  async function handleRegister(event) {
    event?.preventDefault();
    authError = '';
    authSuccess = '';
    try {
      await api.register(registerForm);
      authSuccess = '注册成功，请使用新账户登录';
      registerForm = { username: '', qq: '', password: '' };
      authMode = 'login';
    } catch (err) {
      authError = err.message;
    }
  }

  function handleLoginSuccess(user) {
    currentUser = user;
    profileForm = { username: user.username, qq: user.qq };
    activeSection = 'posts';
    loadDashboard();
  }

  async function loadDashboard() {
    await Promise.all([loadPosts(selectedCategory), loadMyPosts()]);
  }

  async function loadPosts(category) {
    if (!currentUser) return;
    postsError = '';
    loadingPosts = true;
    try {
      const filter = category === '最新' ? null : category;
      const data = await api.listPosts(filter);
      posts = data;
      selectedCategory = category;
    } catch (err) {
      postsError = err.message;
    } finally {
      loadingPosts = false;
    }
  }

  async function loadMyPosts() {
    if (!currentUser) return;
    try {
      myPosts = await api.myPosts();
      myPostsError = '';
    } catch (err) {
      myPostsError = err.message;
    }
  }

  async function submitPost(event) {
    event?.preventDefault();
    composerMessage = '';
    composerError = '';
    if (!composer.title.trim() || !composer.content.trim()) {
      composerError = '请填写标题和内容';
      return;
    }
    if (!CATEGORIES.includes(composer.category)) {
      composerError = '请选择有效的分区';
      return;
    }
    posting = true;
    try {
      await api.createPost({
        title: composer.title.trim(),
        content: composer.content.trim(),
        category: composer.category,
        anonymous: composer.anonymous
      });
      composerMessage = '发布成功';
      composer = { title: '', content: '', category: composer.category, anonymous: false };
      await loadPosts(selectedCategory);
      await loadMyPosts();
    } catch (err) {
      composerError = err.message;
    } finally {
      posting = false;
    }
  }

  async function openPostDetail(post) {
    detailError = '';
    commentMessage = '';
    commentForm = { content: '', anonymous: false };
    try {
      postDetail = await api.postDetail(post.id);
    } catch (err) {
      detailError = err.message;
    }
  }

  function closePostDetail() {
    postDetail = null;
  }

  async function submitComment(event) {
    event?.preventDefault();
    if (!postDetail) return;
    if (!commentForm.content.trim()) {
      commentMessage = '请输入评论内容';
      return;
    }
    commenting = true;
    commentMessage = '';
    try {
      await api.comment(postDetail.id, {
        content: commentForm.content.trim(),
        anonymous: commentForm.anonymous
      });
      commentForm = { content: '', anonymous: commentForm.anonymous };
      commentMessage = '评论已发送';
      postDetail = await api.postDetail(postDetail.id);
      await loadPosts(selectedCategory);
    } catch (err) {
      commentMessage = err.message;
    } finally {
      commenting = false;
    }
  }

  async function handleUpdateProfile(event) {
    event?.preventDefault();
    profileMessage = '';
    profileError = '';
    try {
      const updated = await api.updateProfile({
        username: profileForm.username,
        qq: profileForm.qq
      });
      profileMessage = '信息已更新';
      currentUser = updated;
    } catch (err) {
      profileError = err.message;
    }
  }

  async function handleChangePassword(event) {
    event?.preventDefault();
    passwordMessage = '';
    passwordError = '';
    if (!passwordForm.current_password || !passwordForm.new_password) {
      passwordError = '请填写完整';
      return;
    }
    if (passwordForm.new_password !== passwordForm.confirm) {
      passwordError = '两次输入的新密码不一致';
      return;
    }
    try {
      await api.changePassword({
        current_password: passwordForm.current_password,
        new_password: passwordForm.new_password
      });
      passwordMessage = '密码已修改';
      passwordForm = { current_password: '', new_password: '', confirm: '' };
    } catch (err) {
      passwordError = err.message;
    }
  }

  async function handleLogout() {
    try {
      await api.logout();
    } catch (err) {
      console.warn(err);
    }
    currentUser = null;
    posts = [];
    myPosts = [];
    selectedCategory = '最新';
  }

  async function revealPublicProfile(uid) {
    publicProfileError = '';
    try {
      publicProfile = await api.publicProfile(uid);
    } catch (err) {
      publicProfileError = err.message;
    }
  }

  function closePublicProfile() {
    publicProfile = null;
    publicProfileError = '';
  }

  async function deletePost(postId) {
    if (!window.confirm('确定要删除该帖子吗？')) {
      return;
    }
    try {
      await api.adminDelete(postId);
      await loadPosts(selectedCategory);
    } catch (err) {
      alert(err.message);
    }
  }
</script>

<main>
  {#if !currentUser}
    <section class="auth-shell">
      <div class="auth-card">
        <div class="auth-tabs">
          <button class:active={authMode === 'login'} on:click={() => (authMode = 'login')}>登录</button>
          <button class:active={authMode === 'register'} on:click={() => (authMode = 'register')}>
            注册
          </button>
        </div>
        {#if authMode === 'login'}
          <form class="form-grid" on:submit|preventDefault={handleLogin}>
            <div class="input-field">
              <label for="login-username">用户名</label>
              <input id="login-username" bind:value={loginForm.username} required />
            </div>
            <div class="input-field">
              <label for="login-password">密码</label>
              <input id="login-password" type="password" bind:value={loginForm.password} required />
            </div>
            {#if authError}
              <span class="error-text">{authError}</span>
            {/if}
            {#if authSuccess}
              <span class="success-text">{authSuccess}</span>
            {/if}
            <button class="primary-btn" type="submit">登录</button>
          </form>
        {:else}
          <form class="form-grid" on:submit|preventDefault={handleRegister}>
            <div class="input-field">
              <label for="register-username">用户名</label>
              <input id="register-username" bind:value={registerForm.username} required />
            </div>
            <div class="input-field">
              <label for="register-qq">QQ 号</label>
              <input id="register-qq" bind:value={registerForm.qq} required />
            </div>
            <div class="input-field">
              <label for="register-password">密码</label>
              <input id="register-password" type="password" bind:value={registerForm.password} required minlength="6" />
            </div>
            {#if authError}
              <span class="error-text">{authError}</span>
            {/if}
            <button class="primary-btn" type="submit">注册</button>
          </form>
        {/if}
      </div>
    </section>
  {:else}
    <section class="main-shell">
      <header class="top-bar">
        <div class="top-left">
          <button
            type="button"
            class="tab-pill"
            class:active={activeSection === 'posts'}
            on:click={() => (activeSection = 'posts')}
          >
            帖子
          </button>
          <button
            type="button"
            class="tab-pill"
            class:active={activeSection === 'profile'}
            on:click={() => (activeSection = 'profile')}
          >
            用户空间
          </button>
        </div>
        <div class="inline-list">
          <small>UID: {currentUser.uid}</small>
          <button class="secondary-btn" type="button" on:click={handleLogout}>退出登录</button>
        </div>
      </header>

      {#if activeSection === 'posts'}
        <div class="content-area">
          <section>
            <div class="post-toolbar">
              <label>
                <span>分区筛选</span>
                <input
                  class="board-picker"
                  list="board-options"
                  bind:value={selectedCategory}
                  on:change={(event) => loadPosts(event.currentTarget.value || '最新')}
                />
              </label>
              <datalist id="board-options">
                {#each ALL_BOARDS as board}
                  <option value={board} />
                {/each}
              </datalist>
              <button class="secondary-btn" type="button" on:click={() => loadPosts('最新')}>
                最新
              </button>
            </div>

            <form class="profile-panel" on:submit|preventDefault={submitPost}>
              <h2>发布新帖子</h2>
              <div class="input-field">
                <label for="post-title">标题</label>
                <input id="post-title" bind:value={composer.title} placeholder="写个吸引人的标题吧" />
              </div>
              <div class="input-field">
                <label for="post-category">分区（扩列 / 吐槽 / 表白 / 提问 / 其它）</label>
                <input id="post-category" list="category-options" bind:value={composer.category} />
                <datalist id="category-options">
                  {#each CATEGORIES as cat}
                    <option value={cat} />
                  {/each}
                </datalist>
              </div>
              <div class="input-field">
                <label for="post-content">内容</label>
                <textarea id="post-content" rows="5" bind:value={composer.content} placeholder="说点什么..." />
              </div>
              <label><input type="checkbox" bind:checked={composer.anonymous} /> 匿名发布</label>
              {#if composerError}
                <span class="error-text">{composerError}</span>
              {/if}
              {#if composerMessage}
                <span class="success-text">{composerMessage}</span>
              {/if}
              <button class="primary-btn" type="submit" disabled={posting}>{posting ? '发送中...' : '确认发布'}</button>
            </form>

            {#if postsError}
              <p class="error-text">{postsError}</p>
            {/if}
            {#if loadingPosts}
              <p>加载中...</p>
            {:else if posts.length === 0}
              <p>暂时没有帖子</p>
            {:else}
              <div class="post-grid">
                {#each posts as post}
                  <article class="post-card" on:click={() => openPostDetail(post)}>
                    <div class="category-chip">{post.category}</div>
                    <h3>{post.title}</h3>
                    <p>{post.content}</p>
                    <small>
                      {post.anonymous || !post.author ? '匿名' : `${post.author.username} · ${post.author.qq}`}
                    </small>
                  </article>
                {/each}
              </div>
            {/if}
          </section>

          <aside class="profile-panel">
            <h2>我的帖子</h2>
            {#if myPostsError}
              <p class="error-text">{myPostsError}</p>
            {:else if myPosts.length === 0}
              <p>还没有发布过内容</p>
            {:else}
              <ul>
                {#each myPosts as post}
                  <li>
                  <button class="secondary-btn" type="button" on:click={() => openPostDetail(post)}>
                    {post.title}
                  </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </aside>
        </div>
      {:else}
        <div class="content-area">
          <section class="profile-panel">
            <h2>个人资料</h2>
            <form class="form-grid" on:submit|preventDefault={handleUpdateProfile}>
              <div class="input-field">
                <label for="profile-username">用户名</label>
                <input id="profile-username" bind:value={profileForm.username} />
              </div>
              <div class="input-field">
                <label for="profile-qq">QQ 号</label>
                <input id="profile-qq" bind:value={profileForm.qq} />
              </div>
              {#if profileError}
                <span class="error-text">{profileError}</span>
              {/if}
              {#if profileMessage}
                <span class="success-text">{profileMessage}</span>
              {/if}
              <button class="primary-btn" type="submit">保存资料</button>
            </form>

            <form class="form-grid" on:submit|preventDefault={handleChangePassword}>
              <div class="input-field">
                <label for="current-password">当前密码</label>
                <input id="current-password" type="password" bind:value={passwordForm.current_password} />
              </div>
              <div class="input-field">
                <label for="new-password">新密码</label>
                <input id="new-password" type="password" bind:value={passwordForm.new_password} minlength="6" />
              </div>
              <div class="input-field">
                <label for="confirm-password">确认新密码</label>
                <input id="confirm-password" type="password" bind:value={passwordForm.confirm} minlength="6" />
              </div>
              {#if passwordError}
                <span class="error-text">{passwordError}</span>
              {/if}
              {#if passwordMessage}
                <span class="success-text">{passwordMessage}</span>
              {/if}
              <button class="primary-btn" type="submit">修改密码</button>
            </form>

            <div class="profile-section">
              <h3>历史帖子</h3>
              {#if myPosts.length === 0}
                <p>暂无内容</p>
              {:else}
                <ul>
                  {#each myPosts as post}
                    <li>
                      <button class="secondary-btn" type="button" on:click={() => openPostDetail(post)}>
                        {post.title}
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </section>
          <aside class="profile-panel">
            <h2>提示</h2>
            <p>点击帖子可查看详情并发表评论；若作者实名，可进入其主页查看更多公开信息。</p>
            <p>管理员登录后可在帖子详情中删除违规内容。</p>
          </aside>
        </div>
      {/if}
    </section>
  {/if}

  {#if postDetail}
    <div class="drawer" role="dialog" aria-modal="true">
      <div class="drawer-panel">
        <div class="drawer-header">
          <div>
            <div class="category-chip">{postDetail.category}</div>
            <h2>{postDetail.title}</h2>
            <p>{postDetail.content}</p>
            <small>
              {postDetail.anonymous || !postDetail.author
                ? '匿名'
                : `${postDetail.author.username} · ${postDetail.author.qq}`}
            </small>
            {#if postDetail.author && !postDetail.anonymous}
              <div>
                <button
                  class="secondary-btn"
                  type="button"
                  on:click={() => revealPublicProfile(postDetail.author.uid)}
                >
                  查看 Ta 的主页
                </button>
              </div>
            {/if}
          </div>
          <div class="inline-list">
            {#if currentUser?.is_admin}
              <button class="secondary-btn" type="button" on:click={() => deletePost(postDetail.id)}>
                删除帖子
              </button>
            {/if}
            <button class="secondary-btn" type="button" on:click={closePostDetail}>关闭</button>
          </div>
        </div>

        {#if detailError}
          <p class="error-text">{detailError}</p>
        {/if}

        <section>
          <h3>评论</h3>
          {#if postDetail.comments.length === 0}
            <p>还没有评论</p>
          {:else}
            <div class="comment-list">
              {#each postDetail.comments as comment}
                <div class="comment-item">
                  <p>{comment.content}</p>
                  <small>
                    {comment.anonymous || !comment.author
                      ? '匿名'
                      : `${comment.author.username} · ${comment.author.qq}`}
                  </small>
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <form class="form-grid" on:submit|preventDefault={submitComment}>
          <div class="input-field">
            <label for="comment-content">发表评论</label>
            <textarea id="comment-content" rows="3" bind:value={commentForm.content} />
          </div>
          <label><input type="checkbox" bind:checked={commentForm.anonymous} /> 匿名评论</label>
          {#if commentMessage}
            <span class={commentMessage.includes('评论已发送') ? 'success-text' : 'error-text'}>{commentMessage}</span>
          {/if}
          <button class="primary-btn" type="submit" disabled={commenting}>{commenting ? '发送中...' : '提交评论'}</button>
        </form>
      </div>
    </div>
  {/if}

  {#if publicProfile}
    <div class="public-profile" role="dialog" aria-modal="true">
      <div class="public-profile-card">
        <h3>{publicProfile.username}</h3>
        <p>QQ：{publicProfile.qq}</p>
        <p>UID：{publicProfile.uid}</p>
        <small>加入时间：{publicProfile.joined_at}</small>
        <div>
          <h4>公开帖子</h4>
          {#if publicProfile.posts.length === 0}
            <p>暂无内容</p>
          {:else}
            <ul>
              {#each publicProfile.posts as post}
                <li>
                  <button
                    class="secondary-btn"
                    type="button"
                    on:click={() => {
                      closePublicProfile();
                      openPostDetail(post);
                    }}
                  >
                    {post.title}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
        <button class="primary-btn" type="button" on:click={closePublicProfile}>关闭</button>
      </div>
    </div>
  {:else if publicProfileError}
    <div class="public-profile">
      <div class="public-profile-card">
        <p class="error-text">{publicProfileError}</p>
        <button class="primary-btn" type="button" on:click={closePublicProfile}>关闭</button>
      </div>
    </div>
  {/if}
</main>
