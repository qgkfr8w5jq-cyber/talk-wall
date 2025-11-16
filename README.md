# talk-wall

校园墙 Web 应用，使用 Rust + Axum 提供后端 API，并用 Svelte + Vite 构建单页前端：未登录时显示全屏登录/注册页，登录后可在顶部切换“帖子”和“用户空间”。

## 功能亮点

- **多分区帖子 / 评论**：发帖需填写标题并从“扩列 / 吐槽 / 表白 / 提问 / 其它”中选择分区，可匿名或实名，帖子详情页支持评论。
- **用户空间**：登录后可在“用户空间”中修改用户名、QQ、密码，并查看自己发布过的帖子标题。点击帖子卡片或标题会打开对应详情页。
- **可浏览的公开主页**：实名发布的作者会展示主页入口，主页包含公开资料与其未匿名的帖子列表。
- **管理员操作**：`config.toml` 中配置隐藏 UID 后即可在帖子详情抽屉里执行删帖操作，无需单独后台页面。
- **隐藏 UID**：注册用户会自动生成唯一 UID，用于识别账户和匹配配置文件中的管理员名单。
- **安全认证**：注册需填写用户名、QQ 和密码，密码通过 Argon2 加密，登录状态由 HttpOnly Cookie 维护 7 天。

## 部署教程

下面的流程假设要把后端 + Svelte 前端部署到一台 Linux 服务器，其他平台操作相同：

### 1. 准备依赖

- 安装 [Rust](https://www.rust-lang.org/tools/install)（建议稳定版，附带 `cargo`）。
- 安装 [Node.js](https://nodejs.org/)（推荐 18+ LTS）和 `npm`，用于构建 Svelte 前端。
- 准备 SQLite（项目会自动创建 `talk_wall.db`，无需额外迁移工具）。

### 2. 获取代码

```bash
git clone https://github.com/your-org/talk-wall.git
cd talk-wall
```

### 3. 配置服务

复制或修改根目录的 `config.toml`：

```toml
[server]
addr = "0.0.0.0:8080"  # 服务监听地址

[admins]
uids = ["示例 UID"]  # 管理员隐藏 UID 列表
```

> UID 在用户注册时自动生成，可登录一次、在浏览器的“用户空间”查看 UID，或直接查询数据库 `users` 表中的 `uid` 列后填入。

### 4. 构建前端

后端会直接托管 `frontend/dist` 目录下的静态资源，因此必须先编译 Svelte 应用：

```bash
cd frontend
npm install           # 仅首次或依赖变化时执行
npm run build         # 生成 dist/ 目录
cd ..
```

如需本地联调，可在 `frontend/` 下运行 `npm run dev -- --host`，然后使用 Vite 提供的地址访问页面，API 仍会请求 Axum 后端。

### 5. 构建并运行后端

```bash
cargo build --release   # 生成 target/release/talk-wall
./target/release/talk-wall
```

第一次运行时会在当前目录创建 `talk_wall.db` 并自动建表。若未提前构建前端，Axum 会在日志中提示 `frontend/dist` 缺失，此时回到第 4 步执行构建即可。

### 6. 生产部署建议

- **开机自启**：可为 `target/release/talk-wall` 写一个 systemd service，并在 `ExecStart` 前设置环境变量或工作目录。
- **反向代理**：把服务监听在 `127.0.0.1:8080`，再用 Nginx/Caddy 暴露 HTTPS，静态资源仍由 Axum 提供。
- **数据备份**：周期性复制 `talk_wall.db`，其中包含所有用户、帖子、评论以及 UID。

完成上述步骤后访问 `http://<服务器 IP>:8080/`，即可看到登录页并开始使用帖子、评论、个人空间与管理员删帖等功能。
