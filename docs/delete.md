彻底删除文件
===============

彻底删除文件

[Try it free](https://123yunpan.yuque.com/login?goto=https%3A%2F%2F123yunpan.yuque.com%2Forg-wiki-123yunpan-muaork%2Fcr6ced%2Fsg2gvfk5i3dwoxtg)

彻底删除文件
======

Back to document

API： POST 域名 + /api/v1/file/delete

说明：彻底删除文件前，文件必须要在回收站中，否则无法删除

Header 参数

名称 类型 是否必填 说明
Authorization string 必填 鉴权access_token
Platform string 必填 固定为:open_platform
Body 参数

名称 类型 是否必填 说明
fileIDs array 必填 文件id数组,参数长度最大不超过 100
示例

请求示例

​

Curl

Shell Run Code Copy

99

1

2

3

4

5

6

7

8

9

10

curl--location'https://open-api.123pan.com/api/v1/file/delete' \

--header'Content-Type: application/json' \

--header'Platform: open_platform' \

--header'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJl...(过长省略)' \

--data'{

 "fileIDs": [

 14705301,

 14705306

 ]

}'

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

JSON Copy

9

1

2

3

4

5

6

{

"code": 0,

"message": "ok",

"data": null,

"x-traceID": "dbc1546b-b701-4d30-821a-c29283ffeac5_kong-db-5898fdd8c6-wnv6h"

}

​

If you get gains，please give a like

[123云盘](https://123yunpan.yuque.com/123yunpan)

2025-03-17 11:16

1101

IP region浙江

Report

332Word

[About](https://123yunpan.yuque.com/help/about)[Security](https://123yunpan.yuque.com/about/security)[中文](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/sg2gvfk5i3dwoxtg?language=zh-cn)[Sign up](https://123yunpan.yuque.com/login)

[![Image 1](https://cdn.nlark.com/yuque/0/2023/png/39215739/1697095421529-avatar/0305d093-2687-4529-834f-505f11f1ac50.png?x-oss-process=image%2Fresize%2Cm_fill%2Cw_32%2Ch_32%2Fformat%2Cpng)](https://123yunpan.yuque.com/dashboard)

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

[删除文件至回收站](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/en07662k2kki4bo6)

[彻底删除文件](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/sg2gvfk5i3dwoxtg)

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

[Header 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/sg2gvfk5i3dwoxtg#drenO)

[Body 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/sg2gvfk5i3dwoxtg#jf5bZ)

[示例](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/sg2gvfk5i3dwoxtg#KkSEp)

 Press space bar to start a drag. When dragging you can use the arrow keys to move the item around and escape to cancel. Some screen readers may require you to be in focus mode or to use your pass through key


