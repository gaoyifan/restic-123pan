Title: 下载 · 123云盘

URL Source: https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2

Markdown Content:
下载
===============

下载

[Try it free](https://123yunpan.yuque.com/login?goto=https%3A%2F%2F123yunpan.yuque.com%2Forg-wiki-123yunpan-muaork%2Fcr6ced%2Ffnf60phsushn8ip2)

下载
==

Back to document

API：GET 域名 + /api/v1/file/download_info

Header 参数

名称 类型 是否必填 说明
Authorization string 必填 鉴权access_token
Platform string 必填 固定为:open_platform
QueryString 参数

名称 类型 是否必填 说明
fileId number 是 文件id
返回数据

名称 类型 是否必填 说明
downloadUrl string 是 下载地址
异常返回

code 异常原因 示例message
5113 自用下载流量不足 您今日自用下载流量已超出1GB上限，升级VIP会员可无限流量下载
5066 文件不存在 文件不存在，检查传入fileId是否正确
示例

请求示例

​

Curl

Shell Run Code Copy

9

1

2

3

4

curl--location'https://open-api.123pan.com/api/v1/file/download_info?fileId=14749954' \

--header'Content-Type: application/json' \

--header'Platform: open_platform' \

--header'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJl...(过长省略)' \

​

Java - OkHttp

Java Run Code Copy

​

JavaScript - jQuery

JavaScript Run Code Copy

​

NodeJs - Axios

JavaScript Run Code Copy

​

Python - http.client

Python Run Code Copy

响应示例

​

Shell Run Code Copy

9

1

2

3

4

5

6

7

8

{

"code": 0,

"message": "ok",

"data": {

"downloadUrl": "https://download-cdn.cjjd19.com/123-61/ab6dd0cf/18...(过长省略)"

 },

"x-traceID": "68a2d07c-72d3-4fc6-a0a7-965dfea99dc0_kong-db-5898fdd8c6-wnv6h"

}

​

2 likes

*   ![Image 1: User-63699844](https://cdn.nlark.com/yuque/0/2025/jpeg/anonymous/1765986644554-0c054596-9731-4f94-8b8b-331033ef91da.jpeg?x-oss-process=image%2Fresize%2Cm_fill%2Cw_64%2Ch_64%2Fformat%2Cpng)
*   ![Image 2: 小易](https://cdn.nlark.com/yuque/0/2022/jpeg/29051777/1656880895376-avatar/c2feae42-f19a-4303-ab85-0fbb81ae4caf.jpeg?x-oss-process=image%2Fresize%2Cm_fill%2Cw_64%2Ch_64%2Fformat%2Cpng)

2

[123云盘](https://123yunpan.yuque.com/123yunpan)

2025-03-17 11:17

5847

Report

348Word

[About](https://123yunpan.yuque.com/help/about)[Security](https://123yunpan.yuque.com/about/security)[中文](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2?language=zh-cn)[Sign up](https://123yunpan.yuque.com/login)

[![Image 3](https://cdn.nlark.com/yuque/0/2023/png/39215739/1697095421529-avatar/0305d093-2687-4529-834f-505f11f1ac50.png?x-oss-process=image%2Fresize%2Cm_fill%2Cw_32%2Ch_32%2Fformat%2Cpng)](https://123yunpan.yuque.com/dashboard)

123云盘开放平台

Search⌘ + J

Overview

ToC

简介

[概述](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/ppsuasz6rpioqbyt)

[更新记录](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/ewgaoswrngr1amb1)

接入指南

开发者接入

第三方挂载应用接入

[优秀实践](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/gg705bew0t80ccse)

[💡常见问题](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/ghfd4h0l6c6y6oi8)

API列表

文件管理

上传

重命名

删除

还原

文件详情

文件列表

[移动](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/rsyfsn1gnpgo4m4f)

[下载](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2)

分享管理

离线下载

用户管理

直链

图床

视频转码

Outline

[Header 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2#RKAl5)

[QueryString 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2#KrPSm)

[返回数据](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2#Bkqlb)

[异常返回](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2#H1KRp)

[示例](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2#jVvJ2)

 Press space bar to start a drag. When dragging you can use the arrow keys to move the item around and escape to cancel. Some screen readers may require you to be in focus mode or to use your pass through key


