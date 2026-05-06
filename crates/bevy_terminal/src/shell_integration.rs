//! Shell snippets that emit OSC 133 (`A`/`B`/`C`/`D`) so the terminal
//! can detect command boundaries. Without this, `get_semantic_zones`
//! returns nothing and no command blocks render.

pub fn default_zsh_init() -> &'static str {
    r#"
__bevy_terminal_precmd() { print -Pn "\e]133;A\a"; }
__bevy_terminal_preexec() { print -Pn "\e]133;C\a"; }
typeset -ag precmd_functions preexec_functions
precmd_functions+=(__bevy_terminal_precmd)
preexec_functions+=(__bevy_terminal_preexec)
"#
}

pub fn default_bash_init() -> &'static str {
    r#"
__bevy_terminal_prompt_command() { printf '\e]133;A\a'; }
__bevy_terminal_preexec() {
    if [[ "$BASH_COMMAND" != "$PROMPT_COMMAND" ]]; then
        printf '\e]133;C\a'
    fi
}
PROMPT_COMMAND="__bevy_terminal_prompt_command;${PROMPT_COMMAND:-:}"
trap '__bevy_terminal_preexec' DEBUG
"#
}
