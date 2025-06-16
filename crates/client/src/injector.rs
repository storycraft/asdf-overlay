use core::{mem, time::Duration};
use std::{ffi::OsStr, fs, os::windows::ffi::OsStrExt, path::PathBuf};

use anyhow::{Context, bail};
use goblin::pe::PE;
use ntapi::{
    ntapi_base::CLIENT_ID,
    ntmmapi::{NtAllocateVirtualMemory, NtFreeVirtualMemory, NtWriteVirtualMemory},
    ntpsapi::NtOpenProcess,
    ntrtl::{PUSER_THREAD_START_ROUTINE, RtlCreateUserThread},
};
use scopeguard::defer;
use windows::{
    Wdk::Foundation::OBJECT_ATTRIBUTES,
    Win32::{
        Foundation::{CloseHandle, HANDLE, HMODULE, MAX_PATH, NTSTATUS, WAIT_TIMEOUT},
        System::{
            Memory::{MEM_COMMIT, MEM_RELEASE, PAGE_EXECUTE_READWRITE},
            ProcessStatus::{EnumProcessModulesEx, GetModuleBaseNameA, LIST_MODULES_ALL},
            SystemInformation::{
                GetSystemWow64DirectoryA, IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64,
                IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_UNKNOWN,
            },
            Threading::{
                GetCurrentProcess, GetExitCodeThread, IsWow64Process2, PROCESS_CREATE_THREAD,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
                PROCESS_VM_WRITE, WaitForSingleObject,
            },
        },
    },
    core::PCSTR,
};

#[link(name = "kernel32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn LoadLibraryW(lplibfilename: PCSTR) -> HMODULE;
}

use crate::OverlayDll;

pub fn inject(pid: u32, dll: OverlayDll, timeout: Option<Duration>) -> anyhow::Result<u32> {
    let mut handle = HANDLE(0 as _);
    unsafe {
        let mut attr = OBJECT_ATTRIBUTES {
            Length: mem::size_of::<OBJECT_ATTRIBUTES>() as _,
            ..Default::default()
        };

        // NtOpenProcess is more permissive
        NTSTATUS(NtOpenProcess(
            &mut handle as *mut _ as _,
            (PROCESS_QUERY_LIMITED_INFORMATION
                | PROCESS_CREATE_THREAD
                | PROCESS_VM_OPERATION
                | PROCESS_VM_READ
                | PROCESS_VM_WRITE)
                .0,
            &mut attr as *mut _ as _,
            &mut CLIENT_ID {
                UniqueProcess: pid as _,
                UniqueThread: 0 as _,
            },
        ))
        .ok()
        .context("cannot open process")?;
    };
    defer!(unsafe {
        _ = CloseHandle(handle);
    });

    let target_arch = get_process_arch(handle);
    let current_arch = get_process_arch(unsafe { GetCurrentProcess() });

    let path = match target_arch {
        IMAGE_FILE_MACHINE_AMD64 => dll.x64.context("x64 dll path is not provided")?,
        IMAGE_FILE_MACHINE_I386 => dll.x86.context("x86 dll path is not provided")?,
        IMAGE_FILE_MACHINE_ARM64 => dll.arm64.context("arm64 dll path is not provided")?,
        arch => bail!("Unsupported arch: {}", arch.0),
    };

    execute_remote_fn(
        handle,
        load_library_w_for(handle, target_arch, current_arch)
            .context("cannot find LoadLibraryW")?,
        path.as_os_str(),
        timeout,
    )
}

fn get_process_arch(handle: HANDLE) -> IMAGE_FILE_MACHINE {
    let mut native_output = IMAGE_FILE_MACHINE_UNKNOWN;
    let mut wow64_output = IMAGE_FILE_MACHINE_UNKNOWN;
    unsafe {
        _ = IsWow64Process2(handle, &mut wow64_output, Some(&mut native_output));
    }

    if wow64_output != IMAGE_FILE_MACHINE_UNKNOWN {
        wow64_output
    } else {
        native_output
    }
}

fn load_library_w_for(
    process: HANDLE,
    target_arch: IMAGE_FILE_MACHINE,
    process_arch: IMAGE_FILE_MACHINE,
) -> anyhow::Result<usize> {
    if target_arch == process_arch {
        Ok(LoadLibraryW as usize)
    } else {
        match (process_arch, target_arch) {
            (IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64) => {
                bail!("cannot inject to x64 process from x86 process")
            }

            // wow64 x86
            (_, IMAGE_FILE_MACHINE_I386) => {
                let mut kernel32_path = unsafe {
                    let size = GetSystemWow64DirectoryA(None);
                    let mut buf = vec![0u8; size as _];
                    GetSystemWow64DirectoryA(Some(&mut buf));
                    // pop nul
                    buf.pop();
                    PathBuf::from(str::from_utf8(&buf)?)
                };
                kernel32_path.push("kernel32.dll");

                let data = fs::read(&kernel32_path)?;
                let pe = PE::parse(&data)?;
                let ex = pe
                    .exports
                    .iter()
                    .find(|ex| matches!(ex.name, Some("LoadLibraryW")))
                    .context("cannot find LoadLibraryW exports")?;

                let mod_list = unsafe {
                    let mut size = 0;
                    EnumProcessModulesEx(process, 0 as _, 0, &mut size, LIST_MODULES_ALL)?;
                    let mut list =
                        vec![HMODULE::default(); size as usize / mem::size_of::<HMODULE>()];
                    EnumProcessModulesEx(
                        process,
                        list.as_mut_ptr(),
                        size,
                        &mut size,
                        LIST_MODULES_ALL,
                    )?;
                    list
                };

                let target_kernel32_base = {
                    let mut buf = [0_u8; MAX_PATH as usize + 1];

                    mod_list
                        .into_iter()
                        .find({
                            |module| unsafe {
                                let len = GetModuleBaseNameA(process, Some(*module), &mut buf);
                                str::from_utf8(&buf[..len as usize])
                                    .map(|path| path.eq_ignore_ascii_case("kernel32.dll"))
                                    .unwrap_or(false)
                            }
                        })
                        .context("cannot find kernel32.dll in target process")?
                };

                Ok(ex.rva + target_kernel32_base.0 as usize)
            }

            // x64 on arm64
            (IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_AMD64) => Ok(LoadLibraryW as usize),

            (current_arch, target_arch) => {
                bail!(
                    "Unsupported target arch: {}, current arch: {}",
                    target_arch.0,
                    current_arch.0
                );
            }
        }
    }
}

fn execute_remote_fn(
    process: HANDLE,
    f: usize,
    param: &OsStr,
    timeout: Option<Duration>,
) -> anyhow::Result<u32> {
    let param_encoded = param.encode_wide().collect::<Vec<u16>>();
    let param_encoded = bytemuck::cast_slice::<_, u8>(&param_encoded);

    unsafe {
        let mut base_addr = 0_usize;
        let mut region_size = param_encoded.len();
        // allocate rw page
        NTSTATUS(NtAllocateVirtualMemory(
            process.0 as _,
            &raw mut base_addr as _,
            0,
            &mut region_size,
            MEM_COMMIT.0,
            PAGE_EXECUTE_READWRITE.0,
        ))
        .ok()?;
        // free memory on exit
        defer!({
            let mut base_addr = base_addr;
            _ = NtFreeVirtualMemory(
                process.0 as _,
                &raw mut base_addr as _,
                &mut 0_usize as *mut _,
                MEM_RELEASE.0,
            );
        });

        // write dll path
        NTSTATUS(NtWriteVirtualMemory(
            process.0 as _,
            base_addr as _,
            param_encoded.as_ptr() as _,
            param_encoded.len(),
            0 as _,
        ))
        .ok()?;

        let mut thread_handle: HANDLE = HANDLE::default();
        // create a user thread in the process and execute LoadLibraryW
        NTSTATUS(RtlCreateUserThread(
            process.0 as _,
            0 as _,
            0,
            0,
            0,
            0,
            mem::transmute::<usize, PUSER_THREAD_START_ROUTINE>(f),
            base_addr as _,
            &mut thread_handle as *mut _ as _,
            0 as _,
        ))
        .ok()?;
        // cleanup thread handle
        defer!({
            _ = CloseHandle(thread_handle);
        });

        // wait for overlay dll to start
        let res = WaitForSingleObject(
            thread_handle,
            timeout
                .map(|duration| duration.as_millis() as u32)
                .unwrap_or(u32::MAX),
        );
        if res == WAIT_TIMEOUT {
            bail!("remote thread wait timeout");
        }

        let mut module_handle = 0_u32;
        // Get loaded module handle
        GetExitCodeThread(thread_handle, &mut module_handle)?;
        if module_handle == 0 {
            bail!("failed to load overlay DLL");
        }

        Ok(module_handle)
    }
}
