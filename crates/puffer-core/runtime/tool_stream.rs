use super::ToolOutputDelta;
use std::cell::RefCell;

#[derive(Clone, Copy)]
struct ToolStreamHandler {
    ptr: *mut (),
    call: unsafe fn(*mut (), ToolOutputDelta),
}

thread_local! {
    static TOOL_STREAM_HANDLER: RefCell<Vec<ToolStreamHandler>> = const { RefCell::new(Vec::new()) };
}

struct ToolStreamHandlerGuard;

impl Drop for ToolStreamHandlerGuard {
    fn drop(&mut self) {
        TOOL_STREAM_HANDLER.with(|slot| {
            slot.borrow_mut().pop();
        });
    }
}

pub(crate) fn with_tool_stream_handler<R, F, H>(handler: &mut H, body: F) -> R
where
    F: FnOnce() -> R,
    H: FnMut(ToolOutputDelta),
{
    TOOL_STREAM_HANDLER.with(|slot| {
        slot.borrow_mut().push(ToolStreamHandler {
            ptr: (handler as *mut H).cast(),
            call: call_tool_stream_handler::<H>,
        });
        let _guard = ToolStreamHandlerGuard;
        let result = body();
        result
    })
}

pub(crate) fn emit_tool_stream_delta(delta: ToolOutputDelta) {
    TOOL_STREAM_HANDLER.with(|slot| {
        let handler = slot.borrow().last().copied();
        if let Some(handler) = handler {
            // SAFETY: `with_tool_stream_handler` only installs pointers to
            // stack-local handlers for the dynamic extent of `body()`, and
            // `ToolStreamHandlerGuard` removes the pointer before the stack
            // frame is released.
            unsafe {
                (handler.call)(handler.ptr, delta);
            }
        }
    });
}

unsafe fn call_tool_stream_handler<H>(ptr: *mut (), delta: ToolOutputDelta)
where
    H: FnMut(ToolOutputDelta),
{
    // SAFETY: `ptr` was created from `&mut H` inside `with_tool_stream_handler`
    // and is only invoked while that handler remains on the stack.
    let handler = unsafe { &mut *ptr.cast::<H>() };
    handler(delta);
}
