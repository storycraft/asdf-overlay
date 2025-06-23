use crate::renderer::{dx11::Dx11Renderer, dx12::Dx12Renderer, dx9::Dx9Renderer, vulkan::VulkanRenderer};

pub enum Renderer {
    Dx12(Option<Dx12Renderer>),
    Dx11(Option<Dx11Renderer>),
    Dx9(Option<Dx9Renderer>),
    Opengl,
    Vulkan(Option<VulkanRenderer>),
}
