use super::cell::*;
use super::process::*;
use super::state::*;
use super::value::*;
use crate::util::arc::Arc;

pub extern "C" fn spawn(
    state: &RcState,
    process: &Arc<Process>,
    _this: Value,
    arguments: &[Value],
) -> Result<Return, Value> {
    let new_proc = Process::from_function(arguments[0], &state.config)
        .map_err(|err| process.allocate_string(state, &err))?;
    state.scheduler.schedule(new_proc.clone());
    let new_proc_ptr = process.allocate(Cell::with_prototype(
        CellValue::Process(new_proc),
        state.process_prototype.as_cell(),
    ));
    Ok(Return::Value(new_proc_ptr))
}

pub extern "C" fn send(
    state: &RcState,
    process: &Arc<Process>,
    this: Value,
    arguments: &[Value],
) -> Result<Return, Value> {
    let process = if this == state.process_prototype {
        process.clone()
    } else {
        this.process_value()
            .map_err(|err: String| process.allocate_string(state, &err))?
    };
    let receiver = arguments[0]
        .process_value()
        .map_err(|err| process.allocate_string(state, &err))?;
    if receiver == process {
        receiver.send_message_from_self(
            arguments
                .get(1)
                .map(|x| *x)
                .unwrap_or(Value::from(VTag::Undefined)),
        );
    } else {
        receiver.send_message_from_external_process(arguments[1]);
        attempt_to_reschedule_process(state, &receiver);
    }
    Ok(Return::Value(Value::from(VTag::Undefined)))
}

pub extern "C" fn receive(
    state: &RcState,
    process: &Arc<Process>,
    this: Value,
    _arguments: &[Value],
) -> Result<Return, Value> {
    let process = if this == state.process_prototype {
        process.clone()
    } else {
        this.process_value()
            .map_err(|err: String| process.allocate_string(state, &err))?
    };
    let proc = process;
    if let Some(msg) = proc.receive_message() {
        proc.no_longer_waiting_for_message();
        return Ok(Return::Value(msg));
    } else if proc.is_waiting_for_message() {
        proc.no_longer_waiting_for_message();
        Ok(Return::Value(Value::from(VTag::Null)))
    } else {
        Ok(Return::Value(Value::from(VTag::Null)))
    }
}

pub extern "C" fn wait_for_message(
    state: &RcState,
    process: &Arc<Process>,
    this: Value,
    arguments: &[Value],
) -> Result<Return, Value> {
    let process = if this == state.process_prototype {
        process.clone()
    } else {
        this.process_value()
            .map_err(|err: String| process.allocate_string(state, &err))?
    };
    process.waiting_for_message();
    if let Some(time) = arguments.get(0) {
        if time.is_number() {
            let time = time.to_number();
            if time == std::f64::INFINITY || time == std::f64::NEG_INFINITY || time.is_nan() {
                return Err(process.allocate_string(state, "Trying to sleep for +-inf or NAN time"));
            }
            state
                .timeout_worker
                .suspend(process.clone(), std::time::Duration::from_millis(time as _));
        } else if time.is_cell() {
            match time.as_cell().get().value {
                CellValue::Duration(ref d) => {
                    state.timeout_worker.suspend(process.clone(), d.clone())
                }
                _ => {
                    return Err(
                        process.allocate_string(state, "Expected duration in `wait_for_message`")
                    )
                }
            }
        }
    } else {
        process.suspend_without_timeout();
    }

    if process.has_messages() {
        attempt_to_reschedule_process(state, &process);
    }
    Ok(Return::SuspendProcess)
}

pub extern "C" fn has_messages(
    state: &RcState,
    process: &Arc<Process>,
    this: Value,
    _: &[Value],
) -> Result<Return, Value> {
    let process = if this == state.process_prototype {
        process.clone()
    } else {
        this.process_value()
            .map_err(|err: String| process.allocate_string(state, &err))?
    };
    Ok(Return::Value(Value::from(if process.has_messages() {
        VTag::True
    } else {
        VTag::False
    })))
}

pub extern "C" fn current(
    state: &RcState,
    process: &Arc<Process>,
    _: Value,
    _: &[Value],
) -> Result<Return, Value> {
    Ok(Return::Value(Value::from(process.allocate(
        Cell::with_prototype(
            CellValue::Process(process.clone()),
            state.process_prototype.as_cell(),
        ),
    ))))
}

pub fn initialize_process_prototype(state: &RcState) {
    let proc_prototype = state.process_prototype.as_cell();
    let name = Arc::new("spawn".to_owned());
    let spawn = state.allocate_native_fn_with_name(spawn, name.clone(), 1);
    proc_prototype.add_attribute_without_barrier(&name, Value::from(spawn));
    let name = Arc::new("send".to_owned());
    let send = state.allocate_native_fn_with_name(send, name.clone(), -1);
    proc_prototype.add_attribute_without_barrier(&name, Value::from(send));
    let name = Arc::new("receive_message".to_owned());
    let recv = state.allocate_native_fn_with_name(receive, name.clone(), 0);
    proc_prototype.add_attribute_without_barrier(&name, Value::from(recv));
    let name = Arc::new("wait_for_message".to_owned());
    let wait = state.allocate_native_fn_with_name(wait_for_message, name.clone(), -1);
    proc_prototype.add_attribute_without_barrier(&name, Value::from(wait));
    let name = Arc::new("current".to_owned());
    let current = state.allocate_native_fn_with_name(current, name.clone(), 0);
    proc_prototype.add_attribute_without_barrier(&name, Value::from(current));
    let name = Arc::new("has_messages".to_owned());
    let has_messages = state.allocate_native_fn_with_name(has_messages, name.clone(), 0);
    proc_prototype.add_attribute_without_barrier(&name, Value::from(has_messages));
}

/// Attempts to reschedule the given process after it was sent a message.
fn attempt_to_reschedule_process(state: &RcState, process: &Arc<Process>) {
    // The logic below is necessary as a process' state may change between
    // sending it a message and attempting to reschedule it. Imagine we have two
    // processes: A, and B. A sends B a message, and B waits for a message twice
    // in a row. Now imagine the order of operations to be as follows:
    //
    //     Process A    | Process B
    //     -------------+--------------
    //     send(X)      | receive₁() -> X
    //                  | receive₂()
    //     reschedule() |
    //
    // The second receive() happens before we check the receiver's state to
    // determine if we can reschedule it. As a result we observe the process to
    // be suspended, and would attempt to reschedule it. Without checking if
    // this is actually still necessary, we would wake up the receiving process
    // too early, resulting the second receive() producing a nil object:
    //
    //     Process A    | Process B
    //     -------------+--------------
    //     send(X)      | receive₁() -> X
    //                  | receive₂() -> suspends
    //     reschedule() |
    //                  | receive₂() -> nil
    //
    // The logic below ensures that we only wake up a process when actually
    // necessary, and suspend it again if it didn't receive any messages (taking
    // into account messages it may have received while doing so).
    let reschedule = match process.acquire_rescheduling_rights() {
        RescheduleRights::Failed => false,
        RescheduleRights::Acquired => {
            if process.has_messages() {
                true
            } else {
                process.suspend_without_timeout();

                if process.has_messages() {
                    process.acquire_rescheduling_rights().are_acquired()
                } else {
                    false
                }
            }
        }
        RescheduleRights::AcquiredWithTimeout(timeout) => {
            if process.has_messages() {
                state.timeout_worker.increase_expired_timeouts();
                true
            } else {
                process.suspend_with_timeout(timeout);

                if process.has_messages() {
                    if process.acquire_rescheduling_rights().are_acquired() {
                        state.timeout_worker.increase_expired_timeouts();

                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    };

    if reschedule {
        state.scheduler.schedule(process.clone());
    }
}
