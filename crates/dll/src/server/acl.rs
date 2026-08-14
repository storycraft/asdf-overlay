use scopeguard::defer;
use windows::{
    Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Security::{
            ACL, AllocateAndInitializeSid,
            Authorization::{
                EXPLICIT_ACCESS_A, SET_ACCESS, SetEntriesInAclA, TRUSTEE_A, TRUSTEE_IS_SID,
                TRUSTEE_IS_USER,
            },
            FreeSid, InitializeSecurityDescriptor, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
            SECURITY_DESCRIPTOR, SECURITY_WORLD_SID_AUTHORITY, SetSecurityDescriptorDacl,
        },
        System::SystemServices::{SECURITY_DESCRIPTOR_REVISION, SECURITY_WORLD_RID},
    },
    core::PSTR,
};

/// Create Windows security descriptor allowing read/write permission to Everyone.
pub fn everyone_security_desc() -> anyhow::Result<SECURITY_DESCRIPTOR> {
    let mut everyone_sid = PSID::default();
    unsafe {
        AllocateAndInitializeSid(
            &SECURITY_WORLD_SID_AUTHORITY,
            1,
            SECURITY_WORLD_RID as _,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut everyone_sid,
        )?;
    }
    defer!(unsafe {
        FreeSid(everyone_sid);
    });

    let access = EXPLICIT_ACCESS_A {
        grfAccessPermissions: GENERIC_READ.0 | GENERIC_WRITE.0,
        grfAccessMode: SET_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_A {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: PSTR(everyone_sid.0.cast()),
            ..Default::default()
        },
    };

    let mut pacl: *mut ACL = 0 as _;
    unsafe {
        SetEntriesInAclA(Some(&[access]), None, &mut pacl).ok()?;
    }

    let mut security_desc = SECURITY_DESCRIPTOR::default();
    unsafe {
        InitializeSecurityDescriptor(
            PSECURITY_DESCRIPTOR(&mut security_desc as *mut _ as _),
            SECURITY_DESCRIPTOR_REVISION,
        )?;

        SetSecurityDescriptorDacl(
            PSECURITY_DESCRIPTOR(&mut security_desc as *mut _ as _),
            true,
            Some(pacl),
            false,
        )?;
    }

    Ok(security_desc)
}
