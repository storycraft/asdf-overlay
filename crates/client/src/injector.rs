use core::mem;
use std::{os::windows::ffi::OsStrExt, path::Path};

use anyhow::{Context, bail};
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
        Foundation::{CloseHandle, HANDLE, HMODULE, NTSTATUS},
        System::{
            Memory::{MEM_COMMIT, MEM_RELEASE, PAGE_EXECUTE_READWRITE},
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

use crate::OverlayDll;

#[link(name = "kernel32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn LoadLibraryW(lplibfilename: PCSTR) -> HMODULE;
}

#[inline]
pub fn inject(pid: u32, dll: OverlayDll) -> anyhow::Result<u32> {
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

    let load_library_w = if target_arch == current_arch {
        LoadLibraryW as usize
    } else {
        match (current_arch, target_arch) {
            (IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64) => {
                bail!("cannot inject to x64 process from x86 process")
            }
            (_, IMAGE_FILE_MACHINE_I386) => {
                let mut path = unsafe {
                    let size = GetSystemWow64DirectoryA(None);
                    let mut buf = vec![0u8; size as _];
                    GetSystemWow64DirectoryA(Some(&mut buf));
                    buf
                };
                // remove nul
                path.pop();
                path.extend("\\kernel32.dll\0".bytes());
                todo!()
            }

            (_, IMAGE_FILE_MACHINE_AMD64) => LoadLibraryW as usize,

            (current_arch, target_arch) => {
                bail!(
                    "Unsupported target arch: {}, current arch: {}",
                    target_arch.0,
                    current_arch.0
                );
            }
        }
    };

    let path = match target_arch {
        IMAGE_FILE_MACHINE_AMD64 => dll.x64.context("x64 dll path is not provided")?,
        IMAGE_FILE_MACHINE_I386 => dll.x86.context("x86 dll path is not provided")?,
        IMAGE_FILE_MACHINE_ARM64 => dll.arm64.context("arm64 dll path is not provided")?,
        arch => bail!("Unsupported arch: {}", arch.0),
    };
    unsafe { load_overlay_dll(handle, path, load_library_w) }
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

unsafe fn load_overlay_dll(
    process: HANDLE,
    dll_path: &Path,
    load_library_w: usize,
) -> anyhow::Result<u32> {
    let dll_path_encoded = dll_path.as_os_str().encode_wide().collect::<Vec<u16>>();
    let dll_path_encoded = bytemuck::cast_slice::<_, u8>(&dll_path_encoded);

    unsafe {
        let mut base_addr = 0_usize;
        let mut region_size = dll_path_encoded.len();
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
            dll_path_encoded.as_ptr() as _,
            dll_path_encoded.len(),
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
            mem::transmute::<usize, PUSER_THREAD_START_ROUTINE>(load_library_w),
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
        WaitForSingleObject(thread_handle, u32::MAX);

        let mut module_handle = 0_u32;
        // Get loaded module handle
        GetExitCodeThread(thread_handle, &mut module_handle)?;
        if module_handle == 0 {
            bail!("failed to load overlay DLL");
        }

        Ok(module_handle)
    }
}
