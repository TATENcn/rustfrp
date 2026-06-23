---
doc_id: 05-CICD
version: 1.0.0
last_modified: 2026-06-23
modification_policy: operations
summary: CI 流水线、发布流水线、安全审计、交叉编译矩阵
---

# CI/CD 设计

## 一、CI 流水线（每个 PR 和 push 触发）

### `.github/workflows/ci.yml`

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  # ── Job 1: 静态检查（最快，率先失败） ──
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings

  # ── Job 2: 测试矩阵（跨平台） ──
  test:
    needs: lint
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
        rust: [stable]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-features

  # ── Job 3: 并发测试（SQLite 多线程） ──
  test-concurrent:
    needs: lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p rustfrp-core -- --test-threads=8

  # ── Job 4: 属性测试（大量边界用例） ──
  test-proptest:
    needs: lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: PROPTEST_CASES=10000 cargo test --lib -- proptest

  # ── Job 5: 安全审计 ──
  security:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
      - uses: EmbarkStudios/cargo-deny-action@v1

  # ── Job 6: 前端检查 ──
  frontend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: plugins/gui
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: plugins/gui/package-lock.json
      - run: npm ci
      - run: npm run lint
      - run: npm run typecheck

  # ── Job 7: 性能基准对比（仅 main 分支 push） ──
  bench:
    if: github.ref == 'refs/heads/main'
    needs: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo bench --workspace
```

### 可选的夜间深度测试

```yaml
# .github/workflows/nightly.yml
name: Nightly Deep Tests
on:
  schedule:
    - cron: '37 2 * * *'   # 每天凌晨 2:37（避免 :00 高峰）

jobs:
  # 混沌测试：模拟 frpc 崩溃、网络中断等
  chaos:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -- --ignored  # 运行所有 #[ignore] 的慢速测试

  # 压力测试：1000+ 代理规则生成
  stress:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo bench -- stress
```

---

## 二、发布流水线（tag push 触发）

### `.github/workflows/release.yml`

```yaml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          # ── 客户端目标（含 GUI） ──
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            artifact: rustfrp-manager_linux_amd64
            features: "jemalloc"
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            artifact: rustfrp-manager_windows_amd64.exe
          - target: x86_64-apple-darwin
            os: macos-latest
            artifact: rustfrp-manager_macos_amd64
          - target: aarch64-apple-darwin
            os: macos-latest
            artifact: rustfrp-manager_macos_arm64

          # ── 服务端目标（无 GUI、纯 core、跑在路由器上） ──
          - target: armv7-unknown-linux-gnueabihf
            os: ubuntu-latest
            artifact: rustfrp-core_linux_armv7
            features: "mimalloc-dep"
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
            artifact: rustfrp-core_linux_aarch64
            features: "mimalloc-dep"

    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2

      # 安装交叉编译依赖
      - name: Install cross-compilation deps
        if: contains(matrix.target, 'gnueabihf')
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-arm-linux-gnueabihf

      - name: Install musl tools
        if: contains(matrix.target, 'musl')
        run: |
          sudo apt-get update
          sudo apt-get install -y musl-tools

      # 安装 Tauri 系统依赖（仅桌面目标）
      - name: Install Tauri deps (Linux desktop)
        if: runner.os == 'Linux' && !contains(matrix.target, 'gnueabihf') && !contains(matrix.target, 'musl')
        run: |
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev \
            librsvg2-dev patchelf

      - name: Build
        run: cargo build --release --target ${{ matrix.target }} --features "${{ matrix.features }}"

      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact }}
          path: target/${{ matrix.target }}/release/

  # ── 生成 GitHub Release ──
  release:
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: artifacts
      - name: Package and checksum
        run: |
          cd artifacts
          for d in */; do
            name="${d%/}"
            tar -czf "${name}.tar.gz" "$d"
            sha256sum "${name}.tar.gz" > "${name}.tar.gz.sha256"
          done
      - uses: softprops/action-gh-release@v2
        with:
          files: artifacts/*.tar.gz*
          generate_release_notes: true
```

---

## 三、周期性安全审计

```yaml
# .github/workflows/security-audit.yml
name: Security Audit
on:
  schedule:
    - cron: '13 5 * * *'     # 每天凌晨 5:13
  workflow_dispatch:          # 手动触发

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
      - uses: EmbarkStudios/cargo-deny-action@v1
        with:
          command: check bans licenses sources
```

### 安全告警响应 SLA

| 级别 | 响应时间 | 操作 |
|---|---|---|
| `critical` (RUSTSEC) | 24 小时 | 立即评估 → 升级或打补丁 → Patch Release |
| `high` | 72 小时 | 评估 → 下次 Minor Release 修复 |
| `medium` / `low` | 下一个 Release 窗口 | 评估 → 排入 Backlog |

---

## 四、版本策略

| 事项 | 做法 |
|---|---|
| 版本号 | SemVer（`major.minor.patch`）。发布时手动 `git tag v1.0.0`，不自动升版本 |
| Changelog | 维护 `CHANGELOG.md`，用 [Keep a Changelog](https://keepachangelog.com/) 格式。GitHub Release 自动生成发布说明 |
| FRP 版本兼容 | 每次 CI 拉取最新 FRP 稳定版做兼容性测试。不锁定 FRP 版本——用户可自由选择 |
| Rust MSRV | Latest stable。`rust-toolchain.toml` 指定 |
| 预发布 | 不稳定的功能合入 `next` 分支，tag 打 `v1.0.0-rc.1` |

---

## 五、Dependabot 配置

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
      time: "09:00"

  - package-ecosystem: "npm"
    directory: "/plugins/gui"
    schedule:
      interval: "weekly"
      day: "monday"
      time: "09:00"

  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
      time: "09:00"
```

---

## 六、CI 通过标准

| 检查项 | 阻塞 PR？ |
|---|---|
| `cargo fmt --check` | ✅ 必须通过 |
| `cargo clippy -- -D warnings` | ✅ 必须通过 |
| `cargo test --workspace` 全平台 | ✅ 必须通过 |
| `cargo audit` 无 critical/high | ✅ 必须通过 |
| `cargo deny` 无 error | ✅ 必须通过 |
| `npm run typecheck` | ✅ 必须通过 |
| `npm run lint` | ✅ 必须通过 |
| `cargo bench` 无性能退化 | ⚠️ 仅告警，不阻塞（Phase 1 暂宽松） |
