# 技术文档：Restic REST API 123pan 后端实现

本文档描述 `restic-123pan` 当前实现（普通客户端 API 方案）的整体设计、认证流程、接口映射、缓存与幂等性策略。

## 概述

`restic-123pan` 实现 Restic REST Backend v2，充当转换层：

```
Restic CLI  <--REST API-->  本服务器  <--HTTPS-->  123pan 普通客户端接口
```

由于 123pan OpenAPI 权限关闭，服务端已从 OpenAPI 迁移到普通客户端接口。

## 架构与模块

- `src/restic/handler.rs`
- `src/pan123/client.rs`
- `src/pan123/auth.rs`
- `src/pan123/types.rs`
- `src/pan123/entity.rs`

职责划分：

1. `handler`：实现 Restic 协议语义（init/list/get/put/delete 等）。
2. `client`：封装 123pan 接口调用与重试、上传下载流程、缓存同步。
3. `auth`：负责登录、token 缓存、自动刷新。
4. `entity`：SQLite 持久化文件元数据缓存。

## 认证与签名（迁移重点）

### 认证

当前使用账号密码登录：

- 接口：`POST https://login.123pan.com/api/user/sign_in`
- 请求体：`passport`, `password`, `remember=true`
- 成功码：`code=200`
- 返回字段：`data.token`, `data.expire`

实现细节：

1. `TokenManager::get_token()` 先查内存 token。
2. 内存无可用 token 时查 SQLite `token_cache`。
3. 缓存不可用时触发 `sign_in`。
4. 登录成功后写回内存 + SQLite。
5. 过期前 5 分钟视为过期；登录刷新做 1 分钟限流，避免频繁请求 `sign_in`。

对应代码：

- `src/pan123/auth.rs`

### Web 客户端签名

普通客户端 API 请求需要附加签名 query 参数（OpenList `drivers/123` 同款算法）。

实现逻辑：

1. 取北京时间（UTC+8）格式化 `yyyyMMddHHmm`。
2. 数字按映射表 `adefghlmyijnopkqrstubcvwsz` 做字符替换。
3. 对替换结果做 CRC32 得到 `timeSign`。
4. 计算 `timestamp|random|path|web|3|timeSign` 的 CRC32 得到 `dataSign`。
5. 追加 query：`{timeSign}={timestamp}-{random}-{dataSign}`。

对应代码：

- `Pan123Client::sign_web_url` in `src/pan123/client.rs`

### 公共请求头

所有业务请求统一设置：

- `authorization: Bearer <token>`
- `platform: web`
- `app-version: 3`
- `origin: https://www.123pan.com`
- `referer: https://www.123pan.com/`
- `user-agent: Mozilla/5.0 restic-123pan`

## 123pan API 基础地址

- 登录：`https://login.123pan.com/api/user/sign_in`
- 业务：`https://www.123pan.com/b/api`

## 接口映射（旧 OpenAPI -> 新普通客户端 API）

| 功能 | 旧接口（OpenAPI） | 新接口（普通客户端） |
|---|---|---|
| 认证 | `POST /api/v1/access_token` | `POST https://login.123pan.com/api/user/sign_in` |
| 列目录 | `GET /api/v2/file/list` | `GET /b/api/file/list/new` |
| 建目录 | `POST /upload/v1/file/mkdir` | `POST /b/api/file/upload_request`（`type=1`） |
| 上传（单步） | `POST /upload/v2/file/single/create` | `upload_request -> s3_upload_object/auth -> PUT -> upload_complete/v2` |
| 下载信息 | `GET /api/v1/file/download_info` | `POST /b/api/file/download_info` |
| 移动 | `POST /api/v1/file/move` | `POST /b/api/file/mod_pid` |
| 删除到回收站 | `POST /api/v1/file/trash` | `POST /b/api/file/trash` |
| 彻底删除 | `POST /api/v1/file/delete` | 当前不调用（只走 trash） |

## Restic API 实现详解

### 1. 创建仓库 (`POST /?create=true`)

初始化仓库目录结构：

- 根目录：`PAN123_REPO_PATH`
- 子目录：`data`, `keys`, `locks`, `snapshots`, `index`

目录创建统一使用：

- `POST /b/api/file/upload_request`
- 参数：`type=1`, `fileName`, `parentFileId`, `driveId=0`

若同名目录已存在，先读缓存/远端列表回填并复用。

### 2. 删除仓库 (`DELETE /`)

当前仍未实现，返回 `501 Not Implemented`。

### 3. 获取配置文件状态 (`HEAD /config`)

- 通过仓库根目录缓存/列表定位 `config`。
- 存在时返回 `200` 与 `Content-Length`，不存在返回 `404`。

### 4. 获取配置文件 (`GET /config`)

- 在仓库根目录定位 `config` 文件。
- 调 `download_info` 获取下载入口 URL。
- 解析重定向后下载文件内容并返回。

### 5. 保存配置文件 (`POST /config`)

- 走统一上传链路（`upload_request -> s3 auth -> PUT -> upload_complete/v2`）。
- `duplicate=2` 覆盖同名文件，满足 Restic 重试场景。

### 6. 列出文件 (`GET /{type}/`)

- 通过 SQLite 查缓存。
- 若缓存缺失/强制重建，调用 `file/list/new` 分页拉取并写回缓存。
- `data` 类型按两级目录（前缀）聚合返回。

### 7. 检查文件状态 (`HEAD /{type}/{name}`)

- 在对应目录（或 `data/<prefix>`）查找目标文件。
- 存在返回 `200`，不存在返回 `404`。

### 8. 下载文件 (`GET /{type}/{name}`)

- 先获取下载入口 URL，再解析重定向到真实下载 URL。
- 支持并透传 `Range` 请求头。

### 9. 上传文件 (`POST /{type}/{name}`)

上传流程：

1. `upload_request(type=0, duplicate=2, etag=md5, size, parentFileId)`
2. 若 `reuse=true`，直接拿 `FileId`。
3. 否则调用 `s3_upload_object/auth` 获取预签名 URL。
4. PUT 文件到预签名 URL。
5. 调 `upload_complete/v2` 完成上传。
6. 同步 SQLite（upsert by `parent_id + name`）。

说明：

- 当前实现按单分片路径工作，满足 Restic 当前测试场景。

### 10. 删除文件 (`DELETE /{type}/{name}`)

当前策略：

- 仅调用 `POST /b/api/file/trash`
- 缓存同步删除
- 文件不存在时对上层表现为幂等（返回成功）

## SQLite 持久化缓存

缓存表：`file_nodes`（见 `src/pan123/entity.rs`）

用途：

1. 降低重复目录遍历 API 调用。
2. 提供上传/删除后本地即时一致视图。
3. 服务重启后可复用缓存，加速 warm-up。

缓存维护策略：

- 启动 `warm_cache()`：按目录递归预热。
- 上传成功：upsert 对应文件记录。
- 删除成功：删除对应记录。
- 强制重建：`FORCE_CACHE_REBUILD=true`。

## 错误与重试

统一重试入口：`Pan123Client::retry_api`

- `429`：指数等待（受 `MAX_RETRIES`/`RETRY_DELAY` 控制）
- `401`：触发 token refresh 后重试
- 非成功 code：映射为 `AppError::Pan123Api`

登录刷新同样具备限流与缓存回退，避免频繁调用 `sign_in`。

## 幂等性

- `GET/HEAD`：只读，天然幂等。
- `POST /?create=true`：目录已存在可复用，幂等。
- `POST /{type}/{name}`：`duplicate=2` 覆盖写，幂等。
- `DELETE /{type}/{name}`：不存在时也返回成功语义，幂等。

## 配置

当前关键环境变量：

- `PAN123_USERNAME`
- `PAN123_PASSWORD`
- `PAN123_REPO_PATH`
- `DB_PATH`
- `FORCE_CACHE_REBUILD`

## 验证状态

迁移后测试已覆盖：

1. 单元测试
2. 集成测试（缓存一致性、上传/删除/移动等）
3. E2E（init/backup/restore）
4. 大规模 E2E（100MB）

结论：在普通客户端 API 方案下，当前实现可稳定支撑 Restic v2 备份与恢复流程。
