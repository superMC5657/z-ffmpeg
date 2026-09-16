# 软糖铺客户端接入契约（授权 · 埋点 · 检查更新）

本文档定义客户端与软糖铺服务端的交互协议（激活 / 续验 / 注销激活 / 埋点上报 / 检查更新）。

## 1. 通用约定

- **Base URL**：通过编译期配置 `plugins.softcandy.apiBase` 注入（为空时不联网）。
- **路径前缀**：`/api/v1`；`{slug}` 为产品标识（本应用为 `z-ffmpeg`）。
- **编码与格式**：请求与响应均为 JSON，UTF-8。
- **超时控制**：网络请求统一设置 10 秒超时。
- **统一错误响应**：

```json
{ "error": "ERROR_CODE", "message": "错误提示说明" }
```

## 2. 授权激活模块

### 2.1 基本概念

- **双凭证**：激活与验证使用购买邮箱与 16 位激活码（`XXXX-XXXX-XXXX-XXXX`）。
- **deviceId（机器码）**：系统级稳定指纹（8-128 字符）。
- **授权令牌（license）**：Ed25519 签名的 JWT，客户端本地持久化保存，用于离线验签与宽限期。

### 2.2 激活接口

```http
POST /api/v1/apps/{slug}/activate
Content-Type: application/json
```

请求体：

```json
{
  "code": "SDX4-K9TP-2M7Q-W3HZ",
  "deviceId": "machine-hash-00000001",
  "email": "buyer@example.com",
  "level": "pro"
}
```

成功响应（`200 OK`）：

```json
{
  "license": "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9...",
  "expiresAt": "2026-09-14T12:00:00+08:00",
  "appSlug": "z-ffmpeg",
  "level": "pro",
  "levelLabel": "专业版",
  "deviceId": "machine-hash-00000001"
}
```

错误码：
- 422: `INVALID_REQUEST`, `INVALID_CDK`, `INVALID_DEVICE_ID`, `INVALID_EMAIL`, `APP_MISMATCH`, `CDK_UNAVAILABLE`, `LEVEL_MISMATCH`
- 404: `CDK_NOT_FOUND`, `LEVEL_NOT_FOUND`
- 403: `LEVEL_DISABLED`, `EMAIL_MISMATCH`, `LIMIT_EXCEEDED`

### 2.3 在线验证（续验）

用于启动时和周期性（如每 24 小时）刷新许可证：

```http
POST /api/v1/apps/{slug}/verify
Content-Type: application/json
```

请求体：

```json
{
  "deviceId": "machine-hash-00000001",
  "license": "本地保存的JWT",
  "email": "buyer@example.com"
}
```

成功响应（`200 OK`）：

```json
{
  "valid": true,
  "license": "新令牌",
  "expiresAt": "2026-09-15T12:00:00+08:00"
}
```

客户端行为：
- 成功返回时必须使用新 `license` 覆盖本地保存的旧令牌。
- 收到 401 错误（`CDK_REVOKED` / `DEVICE_NOT_ACTIVATED`）：授权失效，清除本地凭证并切回免费版。
- 网络失败：进入离线宽限期，使用本地公钥离线验签。

### 2.4 离线验签

使用客户端内置的 Ed25519 等级公钥验证本地 JWT：
- 校验签名与算法（EdDSA）
- 校验过期时间 `exp`
- 校验机器码 `deviceId` 是否与本机匹配
- 校验产品标识 `app` 与 `level`

### 2.5 注销激活

解除本机绑定并释放可用设备名额：

```http
POST /api/v1/apps/{slug}/deactivate
Content-Type: application/json
```

请求体：

```json
{
  "code": "SDX4-K9TP-2M7Q-W3HZ",
  "deviceId": "machine-hash-00000001",
  "email": "buyer@example.com"
}
```

成功响应（`200 OK`）：

```json
{ "unbound": true }
```

注销成功后删除本地 `license.json`，并将功能降级为免费版。

## 3. 埋点上报

```http
POST /api/v1/apps/{slug}/analytics
Content-Type: application/json
Authorization: Bearer <analyticsToken>
```

请求体为 JSON 对象，包含设备信息、软件版本及本次会话的编码统计。

行为规范：
- 在应用正常退出或会话结束时上报。
- 上报失败静默处理，不阻塞界面与主流程。

## 4. 检查更新

```http
GET /api/v1/apps/{slug}/latest?platform=windows&channel=stable
```

成功响应（`200 OK`）：

```json
{
  "appSlug": "z-ffmpeg",
  "version": "0.1.3",
  "platform": "windows",
  "channel": "stable",
  "sourceType": "oss_cdn",
  "downloadUrl": "https://example.com/z-ffmpeg/0.1.3/setup.exe",
  "notes": "更新说明",
  "forceUpdate": false,
  "publishedAt": "2026-09-16T12:00:00+08:00"
}
```

行为规范：
- 客户端按分段数字版本号进行比较。
- 检查失败时静默跳过，不阻断应用使用。
