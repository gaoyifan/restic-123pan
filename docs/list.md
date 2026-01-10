获取文件列表（推荐）
===============

获取文件列表（推荐）

[Try it free](https://123yunpan.yuque.com/login?goto=https%3A%2F%2F123yunpan.yuque.com%2Forg-wiki-123yunpan-muaork%2Fcr6ced%2Fzrip9b0ye81zimv4)

获取文件列表（推荐）
==========

Back to document

API： GET 域名 + /api/v2/file/list

注意：此接口查询结果包含回收站的文件，需自行根据字段 trashed 判断处理

Header 参数

名称 类型 是否必填 说明
Authorization string 必填 鉴权access_token
Platform string 必填 固定为:open_platform
QueryString 参数

名称 类型 是否必填 说明
parentFileId number 必填 文件夹ID，根目录传 0
limit number 必填 每页文件数量，最大不超过100
searchData string 选填 搜索关键字将无视文件夹ID参数。将会进行全局查找
searchMode number 选填 0:全文模糊搜索(注:将会根据搜索项分词,查找出相似的匹配项)

1:精准搜索(注:精准搜索需要提供完整的文件名)
lastFileId number 选填 翻页查询时需要填写
返回数据

名称 类型 是否必填 说明
lastFileId number 必填-1代表最后一页（无需再翻页查询）

其他代表下一页开始的文件id，携带到请求参数中
fileList array 必填 文件列表
fileId number 必填 文件Id
filename string 必填 文件名
type number 必填 0-文件 1-文件夹
size number 必填 文件大小
etag string 必填 md5
status number 必填 文件审核状态。 大于 100 为审核驳回文件
parentFileId number 必填 目录ID
category number 必填 文件分类：0-未知 1-音频 2-视频 3-图片
trashed int 必填 文件是否在回收站标识：0 否 1是
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

curl--location'https://open-api.123pan.com/api/v2/file/list?parentFileId=0&limit=100' \

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

JSON Copy

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

11

12

13

14

15

16

17

18

19

20

21

22

23

24

25

26

27

28

29

{

"code": 0,

"message": "ok",

"data": {

"lastFileId": -1,

"fileList": [

{

"fileId": 5373646,

"filename": "download.mp4",

"parentFileId": 14663228,

"type": 0,

"etag": "af..(过长省略)",

"size": 518564433,

"category": 2,

"status": 2,

"punishFlag": 0,

"s3KeyFlag": "x-0",

"storageNode": "m16",

"trashed": 0,

"createAt": "2024-04-30 11:58:36",

"updateAt": "2025-02-24 17:56:45"

},

{

"fileId": 8903127,

"filename": "2.json.gz",

"parentFileId": 14663228,

"type": 0,

"etag": "46..(过长省略)",

"size": 221476024,

​

1 like

*   ![Image 1: Bot-37927](https://cdn.nlark.com/yuque/0/2024/png/26278287/1717753148495-avatar/bf6df594-f094-4d20-beed-b928030a354e.png?x-oss-process=image%2Fresize%2Cm_fill%2Cw_64%2Ch_64%2Fformat%2Cpng)

1

[123云盘](https://123yunpan.yuque.com/123yunpan)

2025-07-09 05:47

6946

IP region浙江

Report

728Word

[About](https://123yunpan.yuque.com/help/about)[Security](https://123yunpan.yuque.com/about/security)[中文](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/zrip9b0ye81zimv4?language=zh-cn)[Sign up](https://123yunpan.yuque.com/login)

[![Image 2](https://cdn.nlark.com/yuque/0/2023/png/39215739/1697095421529-avatar/0305d093-2687-4529-834f-505f11f1ac50.png?x-oss-process=image%2Fresize%2Cm_fill%2Cw_32%2Ch_32%2Fformat%2Cpng)](https://123yunpan.yuque.com/dashboard)

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

[获取文件列表（推荐）](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/zrip9b0ye81zimv4)

[获取文件列表（旧）](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/hosdqqax0knovnm2)

[移动](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/rsyfsn1gnpgo4m4f)

[下载](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/fnf60phsushn8ip2)

分享管理

离线下载

用户管理

直链

图床

视频转码

Outline

[Header 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/zrip9b0ye81zimv4#JEBLM)

[QueryString 参数](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/zrip9b0ye81zimv4#pUTd1)

[返回数据](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/zrip9b0ye81zimv4#xXiWX)

[示例](https://123yunpan.yuque.com/org-wiki-123yunpan-muaork/cr6ced/zrip9b0ye81zimv4#mEgWv)

Adblocker

 Press space bar to start a drag. When dragging you can use the arrow keys to move the item around and escape to cancel. Some screen readers may require you to be in focus mode or to use your pass through key


