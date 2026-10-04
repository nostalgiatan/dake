/// 按错误码和说明给出一条改法。

pub fn hint(code: u32, message: &str) -> String {
    if message.contains("制表符") {
        return "把这一行行首的制表符换成空格".into();
    }
    if message.contains("缩进") {
        return "同一块里用相同个数的空格，块内比块头多缩进一层".into();
    }
    if message.contains("缺少 from") || message.contains("缺少 layout") {
        return "结构里补上 from: str 或 from: bytes，以及 layout".into();
    }
    if message.contains("status 需要") {
        return "set(status, 100 到 599 的整数)。省略时这一次响应是 200".into();
    }
    if message.contains("headers 需要对象") || message.contains("响应头") {
        return "set(headers, object(\"名字\", \"值\"))。值必须是字符串，名字里不要写冒号或换行".into();
    }
    if message.contains("缺少 version") {
        return "lib 写上 version: \"1.0.0\" 这种语义化版本".into();
    }
    if message.contains("缺少 dir") {
        return "repo 写上 dir，指向本机存放包的目录".into();
    }
    if message.contains("密钥文件已存在") {
        return "换一个还不存在的路径。已有的私钥和公钥不会被覆盖".into();
    }
    if message.contains("已存在") {
        return "换一个版本，或使用仓库里已有的那一版。已有的包不会被覆盖".into();
    }
    if message.contains("先提交清单") {
        return "先 PUT /dake/v1/名字/版本/manifest，再上传文件".into();
    }
    if message.contains("超过上限") || message.contains("超过容量") {
        return "提高 repo 的 max_pkgs 或 capacity，或换一个仓库目录".into();
    }
    if message.contains("同一个文件") {
        return "crypto.pair 的私钥和公钥写成两个不同的路径".into();
    }
    if message.contains("参数") {
        return "按声明补齐参数；行为要 set(result, 值) 才有返回值".into();
    }
    if message.contains("找不到") {
        return "确认名字已声明，或 use 了定义它的文件".into();
    }
    if message.contains("只有 str") || message.contains("check 只") {
        return "check 只写在 str 字段上。整数、小数和布尔值用字段类型约束".into();
    }
    if message.contains("default 类型不符") || message.contains("default 需要字面量") {
        return "default 用和字段相同的字面量：str 加引号，int 写整数，float 写小数，bool 写 true 或 false".into();
    }
    if message.contains("未知的验证器") {
        return "check 改用 not_empty、email、url、numeric、alpha 或 alphanumeric".into();
    }
    if message.contains("被替换了两次") {
        return "一个旧结构只保留一个 replaces".into();
    }
    if message.contains("替换成环") {
        return "拆开 replaces 的环，每一层只指向更旧的结构".into();
    }
    if message.contains("需要 default") || message.contains("在旧结构里没有") {
        return "给新字段写 default，或用 take 指向旧结构里已有的字段".into();
    }
    if message.contains("只能写一次") {
        return "这个子句在同一字段或结构里只写一次".into();
    }
    if message.contains("必须是有效的邮箱") {
        return "改成合法邮箱，或去掉该字段的 check: email".into();
    }
    if message.contains("必须是有效的 URL") {
        return "改成合法 URL，或去掉该字段的 check: url".into();
    }
    if message.contains("不能为空") {
        return "写上非空文本，或去掉 check: not_empty".into();
    }
    if message.contains("必须是数字") {
        return "改成数字文本，或去掉 check: numeric".into();
    }
    if message.contains("只能包含字母和数字") {
        return "去掉符号，或去掉 check: alphanumeric".into();
    }
    if message.contains("主机:端口") {
        return "地址写成 127.0.0.1:8080 这种主机和端口，不要写方案".into();
    }
    if message.contains("特异度") || message.contains("路径重复") {
        return "让一条路由更具体，或删掉能接住同一路径的另一条".into();
    }
    if message.contains("不存在") || message.contains("不能读取") || message.contains("不能写入") || message.contains("不能列出") {
        return "补上缺失的文件或目录，或修正读取、写入和列出的权限".into();
    }
    if message.contains("只能包含字母") {
        return "只留字母，或去掉 check: alpha".into();
    }
    let text = match code {
        1004 => "核对 from、layout、sep、order 和字段宽度。order 和宽度只用于 width",
        1005 => "version 写成引号里的语义化版本，例如 \"1.0.0\"",
        1006 => "each 写成 each ${名字} in 列表:。catch 写成 catch as ${名字}:",
        1013 | 1014 | 1017 => "这里需要名字、字符串、数字或 ${变量}。语句关键字只在行首生效",
        3001 => "路径不要包含 ..。包内名字不要包含斜杠",
        3002 => "确认文件已经存在。unpack 指向包目录里的文件名",
        3003 => "密钥文件已存在就换路径。写入失败时确认父目录可写",
        4010 => "字节用 base64 或 utf8 转换。记录和对象用字段读取，不用 str 或 num",
        4011 => "补齐参数个数。list.map 和 list.keep 的行为只能有一个参数",
        4012 => "看清这一项是字符串、整数、小数、字节、列表、记录还是对象",
        4018 => "split 和 json 才能按行 pack。width 只用于字节，并且宽度要合法",
        4030 => "先写 net.url，或写出完整的 http 或 https 地址",
        4031 => "检查地址、端口和 timeout",
        4032 => "补上成对的 cert 和 key，或把 ca 指到签发该服务的证书",
        4033 => "按状态码检查路径和请求体",
        4034 => "请求头和响应头的值写成字符串。status 写成 100 到 599 的整数。timeout 写成正整数",
        4035 => "确认监听地址、端口和目录都可用",
        4036 => "路由路径以 / 开头，并写在对应的 url 或 dir 名字上",
        4037 => "补上缺失的文件或目录，或修正读取、写入和列出的权限",
        _ => "对照语言参考里同一条语句或行为的写法修改这一处",
    };
    text.into()
}

#[cfg(test)]
mod tests {
    use super::hint;

    #[test]
    fn reply_and_key_hints_name_the_repair() {
        assert!(hint(4034, "status 需要 100 到 599 的整数").contains("100 到 599"));
        assert!(hint(4034, "headers 需要对象").contains("object"));
        assert!(hint(3003, "密钥文件已存在").contains("不会被覆盖"));
        assert!(hint(3003, "私钥和公钥不能是同一个文件").contains("两个不同的路径"));
    }
}
