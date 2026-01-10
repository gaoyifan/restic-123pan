单步上传
===============

单步上传

[Try it free](https://123yunpan.yuque.com/login?goto=https%3A%2F%2F123yunpan.yuque.com%2Forg-wiki-123yunpan-muaork%2Fcr6ced%2Fxhiht1uh3yp92pzc)

单步上传
====

Back to document

API： POST 上传域名 + /upload/v2/file/single/create

说明：

●文件名要小于256个字符且不能包含以下任何字符："\/:*?|><

●文件名不能全部是空格

●此接口限制开发者上传单文件大小为1GB

●上传域名是 获取上传域名 接口响应中的域名

●此接口用于实现小文件单步上传一次HTTP请求交互即可完成上传

Header 参数

名称 类型 是否必填 说明
Authorization string 必填 鉴权access_token
Platform string 必填 固定为:open_platform
Body 参数

名称 类型 是否必填 说明
parentFileID number 必填 父目录id，上传到根目录时填写 0
filename string 必填 文件名要小于255个字符且不能包含以下任何字符："\/:*?|><。（注：不能重名）

containDir 为 true 时，传入路径+文件名，例如：/你好/123/测试文件.mp4
etag string 必填 文件md5
size number 必填 文件大小，单位为 byte 字节
file file 必填 文件二进制流
duplicate number 非必填 当有相同文件名时，文件处理策略（1保留两者，新文件名将自动添加后缀，2覆盖原文件）
containDir bool 非必填 上传文件是否包含路径，默认false
返回数据 

名称 类型 是否必填 说明
fileID number 必填 文件ID。当123云盘已有该文件,则会发生秒传。此时会将文件ID字段返回。唯一
completed bool 必填 是否上传完成（如果 completed 为 true 时，则说明上传完成）
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

curl--request POST \

--url https://openapi-upload.123242.com/upload/v2/file/single/create \

--header'Authorization: Bearer eyJhbGciOiJIUzI1...(过长省略)' \

--header'Platform: open_platform' \

--header'content-type: multipart/form-data' \

--form'file=@C:\Users\mfy\Downloads\测试.exe' \

--form parentFileID=11522394 \

--form'filename=测试.exe' \

--form etag=511215951b857390c3f30c17d0dae8ee \

--form size=35763200

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

7

8

9

{

"code": 0,

"message": "ok",

"data": {

"fileID": 11522653,

"completed": true

},

"x-traceID": ""

}

​

If you get gains，please give a like

[123云盘](https://123yunpan.yuque.com/123yunpan)

2025-09-26 11:04

3510

IP region陕西

Report

881Word

[About](https://123yunpan.yuque.com/help/about)[Security](https://123yunpan.yuque.com/about/security)[中文](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xhiht1uh3yp92pzc?language=zh-cn)[Sign up](https://123yunpan.yuque.com/login)

[![Image 1](https://cdn.nlark.com/yuque/0/2023/png/39215739/1697095421529-avatar/0305d093-2687-4529-834f-505f11f1ac50.png?x-oss-process=image%2Fresize%2Cm_fill%2Cw_32%2Ch_32%2Fformat%2Cpng)](https://123yunpan.yuque.com/dashboard)

123云盘开放平台

Search⌘ + J

Overview

ToC

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

[创建目录](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/ouyvcxqg3185zzk4)

V1（旧）

V2（推荐）

[💡上传流程说明](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xogi45g7okqk7svr)

[创建文件](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/txow0iqviqsgotfl)

[上传分片](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/scs8yg89yz8immus)

[上传完毕](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fzzc5o8gok517720)

[获取上传域名](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/agn8lolktbqie7p9)

[单步上传](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xhiht1uh3yp92pzc)

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

[Header 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xhiht1uh3yp92pzc#VfDMR)

[Body 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xhiht1uh3yp92pzc#ohYhF)

[返回数据](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xhiht1uh3yp92pzc#V2Iom)

[示例](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/xhiht1uh3yp92pzc#hpLvA)

Adblocker

 Press space bar to start a drag. When dragging you can use the arrow keys to move the item around and escape to cancel. Some screen readers may require you to be in focus mode or to use your pass through key


