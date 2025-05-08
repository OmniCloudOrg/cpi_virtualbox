// File: cpi_virtualbox/src/lib.rs
use lib_cpi::{
    ActionParameter, ActionDefinition, ActionResult, CpiExtension, ParamType,
    action, param, validation
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;

#[unsafe(no_mangle)]
pub extern "C" fn get_extension() -> *mut dyn CpiExtension {
    Box::into_raw(Box::new(VirtualBoxExtension::new()))
}

/// VirtualBox provider implemented as a dynamic extension
pub struct VirtualBoxExtension {
    name: String,
    provider_type: String,
    default_settings: HashMap<String, Value>,
}

impl VirtualBoxExtension {
    pub fn new() -> Self {
        let mut default_settings = HashMap::new();
        default_settings.insert("os_type".to_string(), json!("Ubuntu_64"));
        default_settings.insert("memory_mb".to_string(), json!(2048));
        default_settings.insert("cpu_count".to_string(), json!(2));
        default_settings.insert("controller_name".to_string(), json!("SATA Controller"));
        default_settings.insert("network_type".to_string(), json!("nat"));
        default_settings.insert("username".to_string(), json!("vboxuser"));
        default_settings.insert("password".to_string(), json!("password"));

        Self {
            name: "virtualbox".to_string(),
            provider_type: "command".to_string(),
            default_settings,
        }
    }
    
    // Helper method to run VBoxManage commands
    fn run_vboxmanage(&self, args: &[&str]) -> Result<String, String> {
        println!("Running VBoxManage command: {:?}", args);
        
        // Only add exe on windows
        #[cfg(target_os = "windows")]
        let output = Command::new("VBoxManage.exe")
            .args(args)
            .output()
            .map_err(|e| format!("Failed to execute VBoxManage command: {}", e))?;
            
        #[cfg(not(target_os = "windows"))]
        let output = Command::new("VBoxManage")
            .args(args)
            .output()
            .map_err(|e| format!("Failed to execute VBoxManage command: {}", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format!("VBoxManage command failed: {}", stderr))
        }
    }
    
    // Define all the methods without the #[action] attribute for now
    
    fn test_install(&self) -> ActionResult {
        let output = self.run_vboxmanage(&["--version"])?;
        
        // Parse the version from the output
        let version = output.trim().to_string();
        
        Ok(json!({
            "success": true,
            "version": version
        }))
    }
    
    fn list_workers(&self) -> ActionResult {
        let output = self.run_vboxmanage(&["list", "vms"])?;
        
        // Parse the output to get VM names and UUIDs
        let mut workers = Vec::new();
        
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            // Each line is in format: "VM Name" {uuid}
            if let (Some(first_quote), Some(last_quote)) = (line.find('"'), line.rfind('"')) {
                if first_quote < last_quote {
                    let name = line[first_quote+1..last_quote].to_string();
                    
                    // Find UUID between curly braces
                    if let (Some(open_brace), Some(close_brace)) = (line.find('{'), line.rfind('}')) {
                        if open_brace < close_brace {
                            let uuid = line[open_brace+1..close_brace].to_string();
                            
                            workers.push(json!({
                                "name": name,
                                "uuid": uuid, // This field is not required by the CPI standard, but ID is. We return it in bolth places for convenience.
                                "id": uuid,
                                "state": "unknown"
                            }));
                            
                            println!("Successfully parsed VM: name='{}', uuid='{}'", name, uuid);
                        }
                    }
                }
            }
        }
        
        // Return just the content for the result object - the CPI wrapper will
        // handle adding the success/error fields
        let result = json!({
            "workers": workers
        });
        
        println!("Final result JSON: {}", result.to_string());
        
        Ok(result)
    }
    
    fn create_worker(&self, worker_name: String, os_type: String, memory_mb: i64, cpu_count: i64) -> ActionResult {
        // Create the VM
        let create_output = self.run_vboxmanage(&[
            "createvm", 
            "--name", &worker_name, 
            "--ostype", &os_type, 
            "--register"
        ])?;
        
        // Extract the UUID
        let mut uuid = String::new();
        for line in create_output.lines() {
            if line.contains("UUID") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    uuid = parts[1].trim().to_string();
                    break;
                }
            }
        }
        
        // Configure memory and CPU
        self.run_vboxmanage(&[
            "modifyvm", 
            &worker_name, 
            "--memory", &memory_mb.to_string(), 
            "--cpus", &cpu_count.to_string()
        ])?;
        
        // Configure network
        self.run_vboxmanage(&[
            "modifyvm", 
            &worker_name, 
            "--nic1", "nat"
        ])?;
        
        Ok(json!({
            "success": true,
            "uuid": uuid,
            "name": worker_name
        }))
    }
    
    fn delete_worker(&self, worker_name: String) -> ActionResult {
        self.run_vboxmanage(&[
            "unregistervm", 
            &worker_name, 
            "--delete"
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn get_worker(&self, worker_name: String) -> ActionResult {
        let output = self.run_vboxmanage(&[
            "showvminfo", 
            &worker_name, 
            "--machinereadable"
        ])?;
        
        let mut vm_info = json!({});
        
        // Parse key properties
        for line in output.lines() {
            let parts: Vec<&str> = line.split('=').collect();
            if parts.len() >= 2 {
                let key = parts[0].trim();
                let value = parts[1].trim().trim_matches('"');
                
                match key {
                    "name" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            obj.insert("name".to_string(), json!(value));
                        }
                    },
                    "UUID" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            obj.insert("id".to_string(), json!(value));
                        }
                    },
                    "VMState" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            obj.insert("state".to_string(), json!(value));
                        }
                    },
                    "memory" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            if let Ok(mem) = value.parse::<i64>() {
                                obj.insert("memory_mb".to_string(), json!(mem));
                            }
                        }
                    },
                    "cpus" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            if let Ok(cpus) = value.parse::<i64>() {
                                obj.insert("cpu_count".to_string(), json!(cpus));
                            }
                        }
                    },
                    "ostype" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            obj.insert("os_type".to_string(), json!(value));
                        }
                    },
                    "firmware" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            obj.insert("firmware".to_string(), json!(value));
                        }
                    },
                    "graphicscontroller" => {
                        if let Some(obj) = vm_info.as_object_mut() {
                            obj.insert("graphics_controller".to_string(), json!(value));
                        }
                    },
                    _ => {}
                }
            }
        }
        
        Ok(json!({
            "success": true,
            "vm": vm_info
        }))
    }
    
    fn has_worker(&self, worker_name: String) -> ActionResult {
        let result = self.run_vboxmanage(&[
            "showvminfo",
            &worker_name,
            "--machinereadable"
        ]);
        
        match result {
            Ok(_) => Ok(json!({
                "success": true,
                "exists": true
            })),
            Err(_) => Ok(json!({
                "success": true,
                "exists": false
            }))
        }
    }
    
    fn start_worker(&self, worker_name: String) -> ActionResult {
        let _output = self.run_vboxmanage(&[
            "startvm",
            &worker_name,
            "--type",
            "headless"
        ])?;
        
        Ok(json!({
            "success": true,
            "started": worker_name
        }))
    }
    
    fn get_volumes(&self) -> ActionResult {
        let output = self.run_vboxmanage(&["list", "hdds"])?;
        
        let blocks = output.split("\n\n").collect::<Vec<&str>>();
        let mut volumes = Vec::new();
        
        for block in blocks {
            if block.trim().is_empty() {
                continue;
            }
            
            let mut volume = json!({});
            let lines = block.lines().collect::<Vec<&str>>();
            
            for line in lines {
                if line.starts_with("UUID:") {
                    if let Some(obj) = volume.as_object_mut() {
                        obj.insert("id".to_string(), json!(line.trim_start_matches("UUID:").trim()));
                    }
                } else if line.starts_with("Location:") {
                    if let Some(obj) = volume.as_object_mut() {
                        obj.insert("path".to_string(), json!(line.trim_start_matches("Location:").trim()));
                    }
                } else if line.starts_with("Capacity:") {
                    let size_str = line.trim_start_matches("Capacity:").trim();
                    let size_parts: Vec<&str> = size_str.split_whitespace().collect();
                    if size_parts.len() >= 2 && size_parts[1] == "MBytes" {
                        if let Ok(size) = size_parts[0].parse::<i64>() {
                            if let Some(obj) = volume.as_object_mut() {
                                obj.insert("size_mb".to_string(), json!(size));
                            }
                        }
                    }
                } else if line.starts_with("Format:") {
                    if let Some(obj) = volume.as_object_mut() {
                        obj.insert("format".to_string(), json!(line.trim_start_matches("Format:").trim()));
                    }
                } else if line.starts_with("Type:") {
                    if let Some(obj) = volume.as_object_mut() {
                        obj.insert("type".to_string(), json!(line.trim_start_matches("Type:").trim()));
                    }
                } else if line.starts_with("Parent UUID:") {
                    if let Some(obj) = volume.as_object_mut() {
                        obj.insert("parent".to_string(), json!(line.trim_start_matches("Parent UUID:").trim()));
                    }
                } else if line.starts_with("State:") {
                    if let Some(obj) = volume.as_object_mut() {
                        obj.insert("state".to_string(), json!(line.trim_start_matches("State:").trim()));
                    }
                }
            }
            
            if !volume.as_object().unwrap().is_empty() {
                volumes.push(volume);
            }
        }
        
        Ok(json!({
            "success": true,
            "volumes": volumes
        }))
    }
    
    fn has_volume(&self, disk_path: String) -> ActionResult {
        let result = self.run_vboxmanage(&[
            "showmediuminfo",
            "disk",
            &disk_path
        ]);
        
        match result {
            Ok(_) => Ok(json!({
                "success": true,
                "exists": true
            })),
            Err(_) => Ok(json!({
                "success": true,
                "exists": false
            }))
        }
    }
    
    fn create_volume(&self, disk_path: String, size_mb: i64) -> ActionResult {
        let output = self.run_vboxmanage(&[
            "createmedium",
            "disk",
            "--filename",
            &disk_path,
            "--size",
            &size_mb.to_string(),
            "--format",
            "VDI"
        ])?;
        
        let mut uuid = String::new();
        let mut path = String::new();
        
        for line in output.lines() {
            if line.contains("UUID:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    uuid = parts[1].trim().to_string();
                }
            } else if line.contains("Location:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    path = parts[1].trim().to_string();
                }
            }
        }
        
        Ok(json!({
            "success": true,
            "uuid": uuid,
            "path": path
        }))
    }
    
    fn delete_volume(&self, disk_path: String) -> ActionResult {
        self.run_vboxmanage(&[
            "closemedium",
            "disk",
            &disk_path,
            "--delete"
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn attach_volume(&self, worker_name: String, controller_name: String, port: i64, disk_path: String) -> ActionResult {
        // Create the storage controller first
        let _ = self.run_vboxmanage(&[
            "storagectl",
            &worker_name,
            "--name",
            &controller_name,
            "--add",
            "sata",
            "--controller",
            "IntelAhci",
            "--portcount",
            "30"
        ]);
        
        // Now attach the disk
        self.run_vboxmanage(&[
            "storageattach",
            &worker_name,
            "--storagectl",
            &controller_name,
            "--port",
            &port.to_string(),
            "--device",
            "0",
            "--type",
            "dvddrive",
            "--medium",
            &disk_path
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn detach_volume(&self, worker_name: String, controller_name: String, port: i64) -> ActionResult {
        self.run_vboxmanage(&[
            "storageattach",
            &worker_name,
            "--storagectl",
            &controller_name,
            "--port",
            &port.to_string(),
            "--device",
            "0",
            "--type",
            "hdd",
            "--medium",
            "none"
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn create_snapshot(&self, worker_name: String, snapshot_name: String) -> ActionResult {
        let output = self.run_vboxmanage(&[
            "snapshot",
            &worker_name,
            "take",
            &snapshot_name
        ])?;
        
        let mut uuid = String::new();
        
        for line in output.lines() {
            if line.contains("taken as") {
                let parts: Vec<&str> = line.split("taken as").collect();
                if parts.len() >= 2 {
                    uuid = parts[1].trim().to_string();
                    break;
                }
            }
        }
        
        Ok(json!({
            "success": true,
            "uuid": uuid
        }))
    }
    
    fn delete_snapshot(&self, worker_name: String, snapshot_name: String) -> ActionResult {
        self.run_vboxmanage(&[
            "snapshot",
            &worker_name,
            "delete",
            &snapshot_name
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn has_snapshot(&self, worker_name: String, snapshot_name: String) -> ActionResult {
        let output = self.run_vboxmanage(&[
            "snapshot",
            &worker_name,
            "list",
            "--machinereadable"
        ])?;
        
        let mut exists = false;
        
        for line in output.lines() {
            if line.contains(&snapshot_name) {
                exists = true;
                break;
            }
        }
        
        Ok(json!({
            "success": true,
            "exists": exists
        }))
    }
    
    fn reboot_worker(&self, worker_name: String) -> ActionResult {
        self.run_vboxmanage(&[
            "controlvm",
            &worker_name,
            "reset"
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn configure_networks(&self, worker_name: String, network_index: i64, network_type: String) -> ActionResult {
        self.run_vboxmanage(&[
            "modifyvm",
            &worker_name,
            &format!("--nic{}", network_index),
            &network_type
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn set_worker_metadata(&self, worker_name: String, key: String, value: String) -> ActionResult {
        self.run_vboxmanage(&[
            "setextradata",
            &worker_name,
            &key,
            &value
        ])?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn snapshot_volume(&self, source_volume_path: String, target_volume_path: String) -> ActionResult {
        let output = self.run_vboxmanage(&[
            "clonemedium",
            "disk",
            &source_volume_path,
            &target_volume_path
        ])?;
        
        let mut uuid = String::new();
        
        for line in output.lines() {
            if line.contains("UUID:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    uuid = parts[1].trim().to_string();
                    break;
                }
            }
        }
        
        Ok(json!({
            "success": true,
            "uuid": uuid
        }))
    }
}

impl CpiExtension for VirtualBoxExtension {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn provider_type(&self) -> &str {
        &self.provider_type
    }
    
    fn list_actions(&self) -> Vec<String> {
        vec![
            "test_install".to_string(),
            "list_workers".to_string(),
            "create_worker".to_string(),
            "delete_worker".to_string(),
            "get_worker".to_string(),
            "has_worker".to_string(),
            "start_worker".to_string(),
            "get_volumes".to_string(),
            "has_volume".to_string(),
            "create_volume".to_string(),
            "delete_volume".to_string(),
            "attach_volume".to_string(),
            "detach_volume".to_string(),
            "create_snapshot".to_string(),
            "delete_snapshot".to_string(),
            "has_snapshot".to_string(),
            "reboot_worker".to_string(),
            "configure_networks".to_string(),
            "set_worker_metadata".to_string(),
            "snapshot_volume".to_string()
        ]
    }
    
    fn get_action_definition(&self, action: &str) -> Option<ActionDefinition> {
        match action {
            "test_install" => Some(ActionDefinition {
                name: "test_install".to_string(),
                description: "Test if VirtualBox is properly installed".to_string(),
                parameters: vec![],
            }),
            "list_workers" => Some(ActionDefinition {
                name: "list_workers".to_string(),
                description: "List all virtual machines".to_string(),
                parameters: vec![],
            }),
            "create_worker" => Some(ActionDefinition {
                name: "create_worker".to_string(),
                description: "Create a new virtual machine".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM to create", ParamType::String, required),
                    param!("os_type", "Operating system type", ParamType::String, optional, json!("Ubuntu_64")),
                    param!("memory_mb", "Memory in MB", ParamType::Integer, optional, json!(2048)),
                    param!("cpu_count", "Number of CPUs", ParamType::Integer, optional, json!(2)),
                ],
            }),
            "delete_worker" => Some(ActionDefinition {
                name: "delete_worker".to_string(),
                description: "Delete a virtual machine".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM to delete", ParamType::String, required),
                ],
            }),
            "get_worker" => Some(ActionDefinition {
                name: "get_worker".to_string(),
                description: "Get information about a virtual machine".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                ],
            }),
            "has_worker" => Some(ActionDefinition {
                name: "has_worker".to_string(),
                description: "Check if a virtual machine exists".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                ],
            }),
            "start_worker" => Some(ActionDefinition {
                name: "start_worker".to_string(),
                description: "Start a virtual machine".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM to start", ParamType::String, required),
                ],
            }),
            "get_volumes" => Some(ActionDefinition {
                name: "get_volumes".to_string(),
                description: "List all virtual disk volumes".to_string(),
                parameters: vec![],
            }),
            "has_volume" => Some(ActionDefinition {
                name: "has_volume".to_string(),
                description: "Check if a disk volume exists".to_string(),
                parameters: vec![
                    param!("disk_path", "Path to the disk", ParamType::String, required),
                ],
            }),
            "create_volume" => Some(ActionDefinition {
                name: "create_volume".to_string(),
                description: "Create a new disk volume".to_string(),
                parameters: vec![
                    param!("disk_path", "Path for the new disk", ParamType::String, required),
                    param!("size_mb", "Size in MB", ParamType::Integer, required),
                ],
            }),
            "delete_volume" => Some(ActionDefinition {
                name: "delete_volume".to_string(),
                description: "Delete a disk volume".to_string(),
                parameters: vec![
                    param!("disk_path", "Path to the disk", ParamType::String, required),
                ],
            }),
            "attach_volume" => Some(ActionDefinition {
                name: "attach_volume".to_string(),
                description: "Create a storage controller and attach a disk to a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("controller_name", "Name of the storage controller", ParamType::String, optional, json!("SATA Controller")),
                    param!("port", "Port number", ParamType::Integer, required),
                    param!("disk_path", "Path to the disk", ParamType::String, required),
                ],
            }),
            "detach_volume" => Some(ActionDefinition {
                name: "detach_volume".to_string(),
                description: "Detach a disk from a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("controller_name", "Name of the storage controller", ParamType::String, optional, json!("SATA Controller")),
                    param!("port", "Port number", ParamType::Integer, required),
                ],
            }),
            "create_snapshot" => Some(ActionDefinition {
                name: "create_snapshot".to_string(),
                description: "Create a snapshot of a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("snapshot_name", "Name of the snapshot", ParamType::String, required),
                ],
            }),
            "delete_snapshot" => Some(ActionDefinition {
                name: "delete_snapshot".to_string(),
                description: "Delete a snapshot of a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("snapshot_name", "Name of the snapshot", ParamType::String, required),
                ],
            }),
            "has_snapshot" => Some(ActionDefinition {
                name: "has_snapshot".to_string(),
                description: "Check if a snapshot exists".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("snapshot_name", "Name of the snapshot", ParamType::String, required),
                ],
            }),
            "reboot_worker" => Some(ActionDefinition {
                name: "reboot_worker".to_string(),
                description: "Reboot a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                ],
            }),
            "configure_networks" => Some(ActionDefinition {
                name: "configure_networks".to_string(),
                description: "Configure network settings for a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("network_index", "Network adapter index", ParamType::Integer, required),
                    param!("network_type", "Network type", ParamType::String, optional, json!("nat")),
                ],
            }),
            "set_worker_metadata" => Some(ActionDefinition {
                name: "set_worker_metadata".to_string(),
                description: "Set metadata for a VM".to_string(),
                parameters: vec![
                    param!("worker_name", "Name of the VM", ParamType::String, required),
                    param!("key", "Metadata key", ParamType::String, required),
                    param!("value", "Metadata value", ParamType::String, required),
                ],
            }),
            "snapshot_volume" => Some(ActionDefinition {
                name: "snapshot_volume".to_string(),
                description: "Clone a disk volume".to_string(),
                parameters: vec![
                    param!("source_volume_path", "Path to the source disk", ParamType::String, required),
                    param!("target_volume_path", "Path for the cloned disk", ParamType::String, required),
                ],
            }),
            _ => None,
        }
    }
    
    fn execute_action(&self, action: &str, params: &HashMap<String, Value>) -> ActionResult {
        match action {
            "test_install" => self.test_install(),
            "list_workers" => self.list_workers(),
            "create_worker" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let os_type = validation::extract_string_opt(params, "os_type")?.unwrap_or_else(|| "Ubuntu_64".to_string());
                let memory_mb = validation::extract_int_opt(params, "memory_mb")?.unwrap_or(2048);
                let cpu_count = validation::extract_int_opt(params, "cpu_count")?.unwrap_or(2);
                
                self.create_worker(worker_name, os_type, memory_mb, cpu_count)
            },
            "delete_worker" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                self.delete_worker(worker_name)
            },
            "get_worker" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                self.get_worker(worker_name)
            },
            "has_worker" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                self.has_worker(worker_name)
            },
            "start_worker" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                self.start_worker(worker_name)
            },
            "get_volumes" => self.get_volumes(),
            "has_volume" => {
                let disk_path = validation::extract_string(params, "disk_path")?;
                self.has_volume(disk_path)
            },
            "create_volume" => {
                let disk_path = validation::extract_string(params, "disk_path")?;
                let size_mb = validation::extract_int(params, "size_mb")?;
                self.create_volume(disk_path, size_mb)
            },
            "delete_volume" => {
                let disk_path = validation::extract_string(params, "disk_path")?;
                self.delete_volume(disk_path)
            },
            "attach_volume" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let controller_name = validation::extract_string_opt(params, "controller_name")?.unwrap_or_else(|| "SATA Controller".to_string());
                let port = validation::extract_int(params, "port")?;
                let disk_path = validation::extract_string(params, "disk_path")?;
                
                self.attach_volume(worker_name, controller_name, port, disk_path)
            },
            "detach_volume" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let controller_name = validation::extract_string_opt(params, "controller_name")?.unwrap_or_else(|| "SATA Controller".to_string());
                let port = validation::extract_int(params, "port")?;
                self.detach_volume(worker_name, controller_name, port)
            },
            "create_snapshot" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let snapshot_name = validation::extract_string(params, "snapshot_name")?;
                self.create_snapshot(worker_name, snapshot_name)
            },
            "delete_snapshot" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let snapshot_name = validation::extract_string(params, "snapshot_name")?;
                self.delete_snapshot(worker_name, snapshot_name)
            },
            "has_snapshot" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let snapshot_name = validation::extract_string(params, "snapshot_name")?;
                self.has_snapshot(worker_name, snapshot_name)
            },
            "reboot_worker" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                self.reboot_worker(worker_name)
            },
            "configure_networks" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let network_index = validation::extract_int(params, "network_index")?;
                let network_type = validation::extract_string_opt(params, "network_type")?.unwrap_or_else(|| "nat".to_string());
                
                self.configure_networks(worker_name, network_index, network_type)
            },
            "set_worker_metadata" => {
                let worker_name = validation::extract_string(params, "worker_name")?;
                let key = validation::extract_string(params, "key")?;
                let value = validation::extract_string(params, "value")?;
                
                self.set_worker_metadata(worker_name, key, value)
            },
            "snapshot_volume" => {
                let source_volume_path = validation::extract_string(params, "source_volume_path")?;
                let target_volume_path = validation::extract_string(params, "target_volume_path")?;
                
                self.snapshot_volume(source_volume_path, target_volume_path)
            },
            _ => Err(format!("Action '{}' not found", action)),
        }
    }
}
