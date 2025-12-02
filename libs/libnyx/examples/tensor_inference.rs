//! Tensor Inference Example
//!
//! Demonstrates allocating tensors and submitting inference requests.

#![no_std]
#![no_main]

use libnyx::prelude::*;

/// Program entry point
#[no_mangle]
pub extern "C" fn _start() -> ! {
    match run_inference() {
        Ok(_) => exit(0),
        Err(_) => exit(1),
    }
}

fn run_inference() -> Result<(), Error> {
    // Define input shape: batch=1, channels=3, height=224, width=224
    let input_shape = TensorShape::tensor4d(1, 3, 224, 224);

    // Define output shape: batch=1, classes=1000
    let output_shape = TensorShape::matrix(1, 1000);

    // Allocate input buffer on GPU (f32 = 4 bytes per element)
    let input = TensorBuffer::alloc_for(&input_shape, DType::F32, Device::Gpu)?;

    // Allocate output buffer on GPU
    let output = TensorBuffer::alloc_for(&output_shape, DType::F32, Device::Gpu)?;

    // In a real program, you would:
    // 1. Map the input buffer to userspace
    // 2. Fill it with image data
    // 3. Submit inference with a real model ID

    // For demonstration, assume model_id is known
    let model_id = 1;

    // Submit inference request (async)
    let request_id = tensor::inference_submit(
        model_id,
        input.id(),
        output.id(),
        tensor::flags::HIGH_PRIORITY,
    )?;

    // In a real program, you would wait for completion via IPC notification
    // then read the output buffer

    // Clean up
    input.free()?;
    output.free()?;

    Ok(())
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(255);
}
