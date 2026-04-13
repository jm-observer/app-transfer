# app-transfer

`app-transfer` 是一个目标程序代理器。它会启动目标程序，并在父进程与子进程之间转发 `stdin`、`stdout`、`stderr`，同时记录转发日志。

## 用法

```bash
app-transfer [args...]
app-transfer --help
```

`[args...]` 会原样透传给被代理的目标程序。

## 目标程序解析顺序

程序按以下优先级查找目标程序：

1. 当前可执行文件同目录下的本地目标文件
   - Windows: `origin.exe`
   - Linux / 非 Windows: `origin`
2. 环境变量 `APP_TARGET`

`APP_TARGET` 支持两种写法：

1. 可执行文件路径
   - 例如：`C:\Program Files\Origin\origin.exe`
   - 例如：`/opt/origin/origin`
2. 通用程序名
   - 例如：`origin.exe`
   - 例如：`origin`

当 `APP_TARGET` 是程序名时，程序会交给系统环境去解析并启动它。

## 帮助参数

`-h` 和 `--help` 优先级最高。

只要命令行中包含这两个参数之一，程序会直接输出帮助信息并退出，不会继续解析目标程序，也不会把该参数透传给被代理程序。

## 示例

Windows PowerShell:

```powershell
$env:APP_TARGET="C:\Program Files\Origin\origin.exe"
.\app-transfer.exe --version
```

Linux shell:

```bash
export APP_TARGET=/opt/origin/origin
./app-transfer --version
```
