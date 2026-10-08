use crate::{
    kernel::{Frame, SharedBudget},
    *,
};
use wasmi::{Caller, ExternType, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, ValType};

fn store_limits(limits: &Limits) -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(limits.memory_bytes)
        .table_elements(limits.table_elements)
        .memories(1)
        .tables(1)
        .instances(1)
        .trap_on_grow_failure(true)
        .build()
}
fn signature(ty: Option<ExternType>, params: &[ValType], results: &[ValType]) -> bool {
    matches!(ty, Some(ExternType::Func(f)) if f.params() == params && f.results() == results)
}

pub(crate) fn validate(kernel: &Kernel, manifest: &Manifest, bytes: &[u8]) -> Result<Module> {
    let module = Module::new(&kernel.engine, bytes).map_err(|_| Error::InvalidModule)?;
    let mut imports = 0;
    for import in module.imports() {
        imports += 1;
        if imports > 1
            || import.module() != "kernel"
            || import.name() != "call"
            || !signature(
                Some(import.ty().clone()),
                &[ValType::I32; 5],
                &[ValType::I32],
            )
        {
            return Err(Error::Signature);
        }
    }
    if !matches!(module.get_export("memory"), Some(ExternType::Memory(m)) if !m.is_64())
        || !signature(module.get_export("alloc"), &[ValType::I32], &[ValType::I32])
    {
        return Err(Error::Signature);
    }
    for name in manifest.exports.keys() {
        if !signature(module.get_export(name), &[ValType::I32; 2], &[ValType::I64]) {
            return Err(Error::Signature);
        }
    }
    for export in module.exports() {
        if export.name() != "memory"
            && export.name() != "alloc"
            && !manifest.exports.contains_key(export.name())
        {
            return Err(Error::Signature);
        }
    }
    // No starts are accepted by the engine. Dry instantiation checks initial
    // memory/table sizes and active data/element bounds before publication.
    let mut store = Store::new(&kernel.engine, store_limits(&kernel.limits));
    store.limiter(|limits| limits);
    let mut linker = Linker::new(&kernel.engine);
    linker
        .func_wrap(
            "kernel",
            "call",
            |_: i32, _: i32, _: i32, _: i32, _: i32| -> i32 { Error::MissingGrant.abi_code() },
        )
        .map_err(|_| Error::Signature)?;
    linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| Error::InvalidModule)?;
    Ok(module)
}

struct Guest<'a> {
    kernel: &'a Kernel,
    frame: Frame,
    budget: SharedBudget,
    limits: StoreLimits,
}
fn range(ptr: i32, len: i32, size: usize, bound: u32) -> Result<std::ops::Range<usize>> {
    let ptr = usize::try_from(ptr).map_err(|_| Error::Buffer)?;
    let len = usize::try_from(len).map_err(|_| Error::Buffer)?;
    if len > bound as usize {
        return Err(Error::Buffer);
    }
    let end = ptr.checked_add(len).ok_or(Error::Buffer)?;
    if end > size {
        return Err(Error::Buffer);
    }
    Ok(ptr..end)
}

fn host_call(
    mut caller: Caller<'_, Guest<'_>>,
    slot: i32,
    reqptr: i32,
    reqlen: i32,
    resptr: i32,
    rescap: i32,
) -> i32 {
    let kernel = caller.data().kernel;
    let frame = caller.data().frame.clone();
    let budget = caller.data().budget.clone();
    let outcome = (|| {
        budget.borrow_mut().fuel = caller.get_fuel().map_err(|_| Error::Trap)?;
        kernel.attempt_slot(&frame, &budget)?;
        let slot = usize::try_from(slot).map_err(|_| Error::InvalidSlot)?;
        let (_, _, _, ep) = kernel.slot_target(&frame, slot)?;
        let memory = caller
            .get_export("memory")
            .and_then(|e| e.into_memory())
            .ok_or(Error::Buffer)?;
        let size = memory.data_size(&caller);
        let req = range(reqptr, reqlen, size, ep.max_request)?;
        let res = range(resptr, rescap, size, kernel.limits.payload_bytes)?;
        if res.len() < ep.max_reply as usize {
            return Err(Error::Buffer);
        }
        let request = memory.data(&caller)[req].to_vec();
        let reply = kernel.call_slot(&frame, slot, &request, &budget)?;
        memory
            .write(&mut caller, res.start, &reply)
            .map_err(|_| Error::Buffer)?;
        i32::try_from(reply.len()).map_err(|_| Error::Buffer)
    })();
    let remaining = budget.borrow().fuel;
    if caller.set_fuel(remaining).is_err() {
        return Error::Trap.abi_code();
    }
    outcome.unwrap_or_else(Error::abi_code)
}

pub(crate) fn execute(
    kernel: &Kernel,
    module: &Module,
    endpoint: &str,
    request: &[u8],
    frame: Frame,
    budget: &SharedBudget,
) -> Result<Vec<u8>> {
    let mut store = Store::new(
        &kernel.engine,
        Guest {
            kernel,
            frame,
            budget: budget.clone(),
            limits: store_limits(&kernel.limits),
        },
    );
    store.limiter(|guest| &mut guest.limits);
    store
        .set_fuel(budget.borrow().fuel)
        .map_err(|_| Error::Trap)?;
    let mut linker = Linker::new(&kernel.engine);
    linker
        .func_wrap("kernel", "call", host_call)
        .map_err(|_| Error::Signature)?;
    let outcome = (|| {
        let instance = linker
            .instantiate_and_start(&mut store, module)
            .map_err(|_| Error::Trap)?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "alloc")
            .map_err(|_| Error::Signature)?;
        let function = instance
            .get_typed_func::<(i32, i32), i64>(&store, endpoint)
            .map_err(|_| Error::Signature)?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or(Error::Signature)?;
        let length = i32::try_from(request.len()).map_err(|_| Error::Buffer)?;
        let ptr = alloc.call(&mut store, length).map_err(|_| Error::Trap)?;
        let input = range(
            ptr,
            length,
            memory.data_size(&store),
            kernel.limits.payload_bytes,
        )?;
        memory
            .write(&mut store, input.start, request)
            .map_err(|_| Error::Buffer)?;
        let packed = function
            .call(&mut store, (ptr, length))
            .map_err(|_| Error::Trap)? as u64;
        let outptr = (packed >> 32) as u32;
        let outlen = packed as u32;
        let ep = &kernel
            .registration(store.data().frame.provider)?
            .manifest
            .exports[endpoint];
        let output = range(
            i32::try_from(outptr).map_err(|_| Error::Buffer)?,
            i32::try_from(outlen).map_err(|_| Error::Buffer)?,
            memory.data_size(&store),
            ep.max_reply,
        )?;
        Ok(memory.data(&store)[output].to_vec())
    })();
    budget.borrow_mut().fuel = store.get_fuel().map_err(|_| Error::Trap)?;
    outcome
}
