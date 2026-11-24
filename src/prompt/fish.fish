# 后台进程只刷新磁盘数据。退出状态和作业数始终从交互式 Fish 获取。
status is-interactive; or return
set -gx VIRTUAL_ENV_DISABLE_PROMPT 1

function __dcx_prompt_postexec --on-event fish_postexec
    set -g __dcx_prompt_status $status
    set -g __dcx_prompt_duration $CMD_DURATION
    set -g __dcx_prompt_after_command 1
end

function __dcx_prompt_repaint --on-signal USR1
    commandline -f repaint 2>/dev/null
end

# 模式标记由提示符本身绘制。
function fish_mode_prompt
end

function fish_prompt --inherit-variable __dcx_prompt_executable
    set -l last_status 0
    set -l duration 0
    set -q __dcx_prompt_status[1]; and set last_status $__dcx_prompt_status
    set -q __dcx_prompt_duration[1]; and set duration $__dcx_prompt_duration
    set -l args --status=$last_status --duration=$duration --jobs=(jobs -g 2>/dev/null | count)
    if set -q __dcx_prompt_after_command
        set -e __dcx_prompt_after_command
        set -a args --after-command
    end
    if contains -- $fish_key_bindings fish_vi_key_bindings fish_hybrid_key_bindings
        set -a args --mode=$fish_bind_mode
    end
    if set -q SSH_CONNECTION
        set -a args --identity="$USER@$hostname"
    else if test "$EUID" = 0
        set -a args --identity="$USER"
    end
    command "$__dcx_prompt_executable" prompt render --shell-pid=$fish_pid --width=$COLUMNS $args
    or printf '%s > ' (prompt_pwd)
end
