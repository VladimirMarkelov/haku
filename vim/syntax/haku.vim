if exists("b:current_syntax")
	finish
endif

let b:current_syntax = "haku"

syntax case ignore

syntax keyword hakuInclude import include contained
syntax match hakuIncludePath "\v\s+.*$" contained
syntax region hakuIncludeRegion start=/\v[ \t@-]*(import|include)/ end=/\v$/ contains=hakuInclude,hakuIncludePath

syntax keyword hakuKeyword error
" Full list
" syntax keyword hakuCond if elseif end done while for in break continue return finish do then
" but some of hakuCond has their own regions
syntax keyword hakuCond if elseif end done while break continue return finish do then

syntax keyword hakuFunction os family platform bit arch feature feat endian
syntax keyword hakuFunction is_file is-file isfile is_dir is-dir isdir
syntax keyword hakuFunction join stem ext dir filename add_ext add-ext with_ext with-ext
syntax keyword hakuFunction with_filename with-filename with_name with-name with_stem with-stem documents docs_dir docs-dir
syntax keyword hakuFunction temp temp_dir temp-dir home home_dir home-dir config config_dir config-dir
syntax keyword hakuFunction print println time format_time format-time time_format time-format
syntax keyword hakuFunction trim trim_left trim-left trim_start trim-start trim_right trim-right trim_end trim-end
syntax keyword hakuFunction starts_with starts-with ends_with ends-with lowcase upcase replace match substr
syntax keyword hakuFunction pad_center pad-center pad_left pad-left pad_right pad-right
syntax keyword hakuFunction field fields field_sep field-sep fields_sep fields-sep rand_str rand-str
syntax keyword hakuFunction inc dec shell invoke_dir invoke-dir invokedir glob
syntax keyword hakuFunction set_env set-env setenv del_env del-env delenv clear_env clear-env clearenv
syntax keyword hakuFunction ver_inc ver-inc ver_match ver-match ver_eq ver-eq ver_gt ver-gt ver_lt ver-lt
syntax match hakuFunction "\vcontains"

syntax keyword hakuAttributeName os family platform bit arch feature feat endian contained
syntax match hakuAttribute "\v^\s*\#\[.*\]$" contains=hakuAttributeName
"
syntax match hakuDocComment "\v^\s*##.*$" contains=hakuExecString
syntax region hakuComment start=/\v^\s*\/\// end=/\v$/ contains=hakuExecString
syntax region hakuComment start=/\v^\s*#[^#\[]/ end=/\v$/ contains=hakuExecString

syntax match hakuNumber "\v\d+"
syntax match hakuExecSpecial   "\v^\s*[@-]+"

syntax keyword hakuOperator and or not
syntax match hakuOperator "\v\!"
syntax match hakuOperator "\v\="
syntax match hakuOperator "\v\<"
syntax match hakuOperator "\v\>"
syntax match hakuOperator "\v\?"
syntax match hakuOperator "\v\?\="
syntax match hakuOperator "\v\=\="
syntax match hakuOperator "\v\>\="
syntax match hakuOperator "\v\<\="
syntax match hakuOperator "\v\&\&"
syntax match hakuOperator "\v\|\|"

syntax match hakuVar "\v\$[0-0a-zA-Z_-]+"
syntax match hakuInnerVar "\v\$\{[0-0a-zA-Z_-]+\}"

syntax region hakuString start=/\v"/ skip=/\v\\./ end=/\v"/ contains=hakuInnerVar
syntax region hakuString start=/\v'/ skip=/\v\\./ end=/\v'/ contains=hakuInnerVar
syntax match hakuExecString "\v\`[^\`]*\`" contains=hakuInnerVar

syntax match hakuRecipeFlags "\v^\s*[@-]*" contained
syntax match hakuRecipeName "\v^[ \t@-]*[0-9a-zA-Z_-]+" contains=hakuRecipeFlags contained
syntax match hakuRecipeDelimiter "\v:" contained
syntax match hakuRecipe "\v^[ \t\+0-9a-zA-Z@_-]+:[ \t0-9a-zA-Z_-]*$" contains=hakuRecipeName,hakuRecipeDelimiter

syntax region hakuFor transparent start=/\v\s*for\s/ end=/\v$/ contains=hakuForAndVarStmt,hakuForIn,hakuString,hakuVar,hakuInnerVar,hakuExecString,hakuFunction
syntax match hakuForAndVarStmt "\v\s*for\s+[0-9a-zA-Z_-]+" contains=hakuForStmt contained
syntax keyword hakuForStmt for contained nextgroup=hakuForVar
syntax keyword hakuForIn in contained
syntax match hakuForVar "\v\s[0-9a-zA-Z_-]+" contained

syntax region hakuAssign transparent start=/\v\s*[0-9a-zA-Z_-]+\s*\=/ end=/\v$/ contains=hakuVar,hakuOperator,hakuString,hakuExecString,hakuAssignVar,hakuFunction
syntax match hakuAssignVar "\v^\s*[0-9a-zA-Z_-]+" contained

highlight link hakuForStmt Statement
highlight link hakuForVar Identifier
highlight link hakuForIn Statement
highlight link hakuAssignVar Identifier

highlight link hakuComment Comment
highlight link hakuDocComment SpecialComment
highlight link hakuOperator Operator
highlight link hakuString String
highlight link hakuFunction Function
highlight link hakuKeyword Statement
highlight link hakuAttributeName Identifier
highlight link hakuAttribute Comment
highlight link hakuVar Identifier
highlight link hakuInnerVar Identifier
highlight link hakuCond Conditional
highlight link hakuExecString Include
highlight link hakuNumber Number
highlight link hakuExecSpecial SpecialComment
highlight link hakuRecipeDelimiter Character
highlight link hakuRecipeName Label
highlight link hakuRecipeFlags SpecialComment
highlight link hakuInclude Include
highlight link hakuIncludePath String
