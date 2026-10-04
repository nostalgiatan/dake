use std::sync::Arc;
use super::executor::{ExecutionError, Executor};
use crate::dsl::ast::Value;
use crate::dsl::ir::CallArg;
use sysinfo::{Disks, System};

pub(super) fn call(exec: &mut Executor, name: &str, args: &[CallArg]) -> Result<Value, ExecutionError> {
    match name {
        "sys.host" => {
            none(args)?;
            Ok(object([
                ("name", Value::String(System::host_name().unwrap_or_default())),
                ("os", Value::String(System::name().unwrap_or_default())),
                ("version", Value::String(System::os_version().unwrap_or_default())),
                ("kernel", Value::String(System::kernel_version().unwrap_or_default())),
            ]))
        }
        "sys.cpu" => {
            none(args)?;
            let mut system = System::new();
            system.refresh_cpu_all();
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            system.refresh_cpu_all();
            Ok(object([
                ("usage", Value::Float(f64::from(system.global_cpu_usage()))),
                ("logical", Value::Number(i64::try_from(system.cpus().len()).unwrap_or(0))),
                ("physical", Value::Number(i64::try_from(System::physical_core_count().unwrap_or(0)).unwrap_or(0))),
            ]))
        }
        "sys.mem" => {
            none(args)?;
            let mut system = System::new();
            system.refresh_memory();
            Ok(object([
                ("used", number(system.used_memory())?),
                ("total", number(system.total_memory())?),
                ("swap_used", number(system.used_swap())?),
                ("swap_total", number(system.total_swap())?),
            ]))
        }
        "sys.disk" => {
            exact(args, 1)?;
            let path = exec.arg_string(args, 0)?;
            let full = std::fs::canonicalize(&path).map_err(|err| ExecutionError::new(3002, format!("{path}: {err}")))?;
            let disks = Disks::new_with_refreshed_list();
            let disk = disks
                .list()
                .iter()
                .filter(|disk| full.starts_with(disk.mount_point()))
                .max_by_key(|disk| disk.mount_point().as_os_str().len())
                .ok_or_else(|| ExecutionError::new(3002, format!("{path}: 找不到所在文件系统")))?;
            Ok(disk_object(disk))
        }
        "sys.disks" => {
            none(args)?;
            let disks = Disks::new_with_refreshed_list();
            let items = disks.list().iter().map(disk_object).collect();
            Ok(Value::List(Arc::new(items)))
        }
        _ => Err(ExecutionError::new(4005, format!("未知的内置行为: {name}"))),
    }
}

fn disk_object(disk: &sysinfo::Disk) -> Value {
    let name = disk.name().to_string_lossy().into_owned();
    let mount = disk.mount_point().to_string_lossy().into_owned();
    object([
        ("name", Value::String(name)),
        ("mount", Value::String(mount)),
        ("free", number(disk.available_space()).unwrap_or(Value::Number(0))),
        ("total", number(disk.total_space()).unwrap_or(Value::Number(0))),
    ])
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(Arc::new(fields.into_iter().map(|(key, value)| (key.to_string(), value)).collect()))
}

fn number(bytes: u64) -> Result<Value, ExecutionError> {
    i64::try_from(bytes)
        .map(Value::Number)
        .map_err(|_| ExecutionError::new(4010, "字节数超出整数范围".into()))
}

fn none(args: &[CallArg]) -> Result<(), ExecutionError> {
    exact(args, 0)
}

fn exact(args: &[CallArg], n: usize) -> Result<(), ExecutionError> {
    if args.len() == n {
        Ok(())
    } else {
        Err(ExecutionError::new(4011, format!("需要 {n} 个参数，得到 {}", args.len())))
    }
}
