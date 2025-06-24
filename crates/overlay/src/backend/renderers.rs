use crate::renderer::{
    dx9::Dx9Renderer, dx11::Dx11Renderer, dx12::Dx12Renderer, vulkan::VulkanRenderer,
};

pub enum Renderer {
    Dx12(Option<Dx12Renderer>),
    Dx11(Option<Dx11Renderer>),
    Dx9(Option<Dx9Renderer>),
    Opengl,
    Vulkan(Option<Box<VulkanRenderer>>),
}
