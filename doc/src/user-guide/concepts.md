# Concepts
To expose consistent interface from the target process, the library introduces some concepts.
These concepts are important as they represents how Asdf Overlay hook and abstracts underlying rendering infrastructure.

## Window
Window represents a Win32 window with an HWND.
Each window can accept inputs and have one Swapchain.

## Swapchain
Swapchain represents collection of framebuffers associated to an window.
The actual implementation varies by GPU backends.

For each swapchain, Asdf Overlay accept any BGRA formatted Direct3D 11 shared texture to be set as overlay surface.
Provided Direct3D 11 texture is drew on top of actual framebuffer before presentation.

Overlay texture must be drew using same GPU with target swapchain as it uses shared texture to avoid expensive cpu copy.
If provided texture is rendered on other GPU, the rendering will fail and overlay will not be displayed.
This is not significant issue on desktop as most desktops have only one GPU.
However some laptops have integrated GPU and discrete one then it can be problem.

## Input
Input is mouse and keyboard inputs coming from window.
This also includes raw inputs coming from event loop associated to a window.

Asdf Overlay let you listen these inputs, intercept and block them so users can interact with overlay.
To prevent abuse, blocking is disabled when the user attempts to close the window.
