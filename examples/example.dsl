use "rules/note.dake" as rules

set(project, "example")
set(version, "0.3.0")

lib:
    name: "example-lib"
    version: "1.0.0"
    desc: "示例数据包"
    repo: "https://github.com/example/repo"
    keywords: ["data", "package"]
    readme: "README.md"
    out_dir: "output"

print("开始 ${project}")

if ${version} == "0.3.0":
    print("版本匹配")
else:
    print("其他版本")

rules.greet(${project})
