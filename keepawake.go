// keepawake.go - Windows 防锁屏/防睡眠/保持网络小工具（Go 单文件版，无第三方依赖）
//
// 原理同 Rust 版：
//  1. SetThreadExecutionState(ES_SYSTEM|ES_DISPLAY|ES_AWAYMODE) 防睡眠/常亮/保网络
//  2. 每 4 分钟按一次无害的 F15 键，重置空闲计时，对付 GPO 强制锁屏
//
// 编译（需安装 Go 1.21+）：
//   go mod init keepawake
//   go build -o keepawake.exe keepawake.go
//
// 运行：
//   .\keepawake.exe
package main

import (
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"
	"time"
)

const (
	ES_CONTINUOUS       = 0x80000000
	ES_SYSTEM_REQUIRED  = 0x00000001
	ES_DISPLAY_REQUIRED = 0x00000002
	ES_AWAYMODE_REQUIRED = 0x00000040
	VK_F15              = 0x7E
	KEYEVENTF_KEYUP     = 0x0002
)

var (
	kernel32                 = syscall.NewLazyDLL("kernel32.dll")
	procSetThreadExecState   = kernel32.NewProc("SetThreadExecutionState")
	user32                   = syscall.NewLazyDLL("user32.dll")
	procKeybdEvent           = user32.NewProc("keybd_event")
)

func setAwake(display, away bool) {
	flags := uint32(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)
	if display {
		flags |= ES_DISPLAY_REQUIRED
	}
	if away {
		flags |= ES_AWAYMODE_REQUIRED
	}
	procSetThreadExecState.Call(uintptr(flags))
}

func clearAwake() {
	procSetThreadExecState.Call(uintptr(ES_CONTINUOUS))
}

func jiggleF15() {
	procKeybdEvent.Call(uintptr(VK_F15), 0, 0, 0)
	time.Sleep(50 * time.Millisecond)
	procKeybdEvent.Call(uintptr(VK_F15), 0, uintptr(KEYEVENTF_KEYUP), 0)
}

func main() {
	noDisplay := flag.Bool("no-display", false, "不阻止关闭显示器")
	noAway := flag.Bool("no-away", false, "不使用 AwayMode")
	noJiggle := flag.Bool("no-jiggle", false, "不模拟 F15 按键")
	interval := flag.Int("interval", 30, "ExecutionState 刷新间隔（秒）")
	jiggleInterval := flag.Int("jiggle-interval", 240, "F15 按键间隔（秒）")
	flag.Parse()

	if *interval < 5 {
		*interval = 5
	}
	if *jiggleInterval < 30 {
		*jiggleInterval = 30
	}
	display := !*noDisplay
	away := !*noAway
	jiggle := !*noJiggle

	// Ctrl+C 退出时恢复电源策略
	c := make(chan os.Signal, 1)
	signal.Notify(c, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-c
		clearAwake()
		fmt.Println("\n已恢复系统电源策略，再见。")
		os.Exit(0)
	}()

	fmt.Println("=== keepawake 运行中 ===")
	fmt.Printf("刷新间隔: %ds，按 Ctrl+C 退出。\n", *interval)
	setAwake(display, away)

	elapsed := 0
	for {
		time.Sleep(time.Duration(*interval) * time.Second)
		setAwake(display, away)
		elapsed += *interval
		if jiggle && elapsed >= *jiggleInterval {
			elapsed = 0
			jiggleF15()
			fmt.Printf("[%s] F15 心跳，保持在线。\n", time.Now().Format("15:04:05"))
		}
	}
}
