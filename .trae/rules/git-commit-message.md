---
alwaysApply: true
scene: git_message
---

确保提交信息符合以下格式：

<type>(<scope>): <subject>
<BLANK LINE>

<body>
<BLANK LINE>
<footer>

### 2. Header（必填项）

Header 包含三个部分：

- **type（类型，必填）**：表示本次提交的目的。常用类型包括：
  - `feat`：新增功能（Feature）
  - `fix`：修复 Bug
  - `docs`：文档更新（Documentation）
  - `style`：代码格式调整（不影响代码运行逻辑，如空格、缩进等）
  - `refactor`：代码重构（既不是新增功能，也不是修复 Bug）
  - `perf`：性能优化（Performance）
  - `test`：增加或修改测试用例
  - `chore`：构建过程、辅助工具或依赖变更（Chore）
  - `revert`：回滚之前的提交
- **scope（作用范围，选填）**：用于说明本次提交影响的模块、组件或页面（如 `user`、`login`、`api`）。
- **subject（简短描述，必填）**：对本次提交的简明总结。建议以动词开头（如 "Add", "Fix"），控制在 50 个字符以内，且结尾不加句号。
- **语言要求**：Header 部分（包含 type、scope 和 subject）**必须使用英文描述，避免使用中文**。

### 3. Body 与 Footer（选填项）

- **Body（正文）**：与 Header 之间需空一行。主要用于详细说明本次修改的**原因（Why）**和具体实现逻辑，而不是仅仅描述做了什么（代码本身已体现）。建议每行不超过 72 个字符。
- **Footer（页脚）**：主要用于关联 Issue 或声明不兼容变更。例如：
  - 关闭或关联 Issue：`Closes #123` 或 `Fixes #45`
  - 破坏性变更：以 `BREAKING CHANGE:` 开头，说明与旧版本不兼容的变动。
- **语言要求**：Body 与 Footer 部分**必须使用中文描述，避免使用英文**（注：系统关键字如 `Closes`、`BREAKING CHANGE` 等保留英文）。
- **Body格式**：如果分条分点，使用 1. 数字序号格式，而不是 - 符号。

### 4. 良好实践示例

- **常规提交**：`feat(user): add user registration feature`
- **Bug 修复**：`fix(login): fix the issue where expired captcha does not prompt`
- **包含正文与页脚的完整提交**：

  ```text
  feat(user): add user login interface

  - 新增 /api/user/login 路由
  - 增加 JWT 生成工具类
  - 在 UserService 中增加密码校验方法

  Closes #123
  ```

为了保证规范的有效落地，团队通常会引入如 Husky + Commitlint 等工程化工具，在提交阶段自动拦截不符合规范的 Message。
