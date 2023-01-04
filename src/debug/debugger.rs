use std::collections::HashSet;
use std::sync::Mutex;
use crate::cpu::{Memory, State};
use crate::cpu::error::Error;
use crate::debug::debugger::DebuggerMode::{Breakpoint, Finished, Invalid, Paused, Running};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DebuggerMode {
    Running,
    Invalid(Error),
    Paused,
    Breakpoint,
    Finished(u32),
}

pub struct ExecutableRange {
    pub address: u32,
    pub count: u32
}

impl ExecutableRange {
    pub fn contains(&self, address: u32) -> bool {
        let end_bound = self.address.checked_add(self.count)
            .map(|end_bound| address < end_bound)
            .unwrap_or(true);

        address >= self.address && end_bound
    }
}

pub struct Debugger<Mem: Memory> {
    mode: DebuggerMode,

    state: State<Mem>,
    batch: usize,

    executable: Option<Vec<ExecutableRange>>
}

// Addresses
type Breakpoints = HashSet<u32>;

#[derive(Debug)]
pub struct DebugFrame {
    pub mode: DebuggerMode,

    pub pc: u32,
    pub registers: [u32; 32],
    pub lo: u32,
    pub hi: u32
}

impl<Mem: Memory> Debugger<Mem> {
    pub fn new(state: State<Mem>) -> Debugger<Mem> {
        Debugger { mode: Paused, state, batch: 140, executable: None }
    }

    pub fn new_with_ranges(state: State<Mem>, executable: Vec<ExecutableRange>) -> Debugger<Mem> {
        Debugger { mode: Paused, state, batch: 140, executable: Some(executable) }
    }

    fn frame_with_pc(&self, pc: u32) -> DebugFrame {
        DebugFrame {
            mode: self.mode,
            pc,
            registers: self.state.registers,
            lo: self.state.lo,
            hi: self.state.hi,
        }
    }

    pub fn frame(&self) -> DebugFrame {
        self.frame_with_pc(self.state.pc)
    }

    pub fn state(&mut self) -> &mut State<Mem> {
        &mut self.state
    }

    pub fn memory(&mut self) -> &mut Mem {
        &mut self.state.memory
    }

    pub fn cycle(&mut self, breakpoints: &Breakpoints, hit_breakpoint: bool) -> Option<DebugFrame> {
        if !hit_breakpoint && breakpoints.contains(&self.state.pc) {
            self.mode = Breakpoint;

            return Some(self.frame())
        }

        let start_pc = self.state.pc;

        if let Err(err) = self.state.step() {
            let invalid_or_unmapped = match err {
                Error::CpuInvalid(_) => true,
                Error::MemoryUnmapped(_) => true,
                _ => false
            };

            let is_executable = self.executable.as_ref()
                .map(|executable| executable.iter()
                    .any(|x| x.contains(start_pc)))
                .unwrap_or(true);

            self.mode = if !is_executable && invalid_or_unmapped {
                Finished(start_pc)
            } else {
                Invalid(err)
            };

            Some(self.frame_with_pc(start_pc))
        } else {
            None
        }
    }

    pub fn pause(&mut self) {
        self.mode = Paused
    }

    pub fn run(debugger: &Mutex<Debugger<Mem>>, breakpoints: &Breakpoints) -> DebugFrame {
        let mut hit_breakpoint = {
            let mut value = debugger.lock().unwrap();

            if value.mode == Running {
                return value.frame()
            }

            let result = value.mode;
            value.mode = Running;

            result == Breakpoint
        };

        loop {
            let mut value = debugger.lock().unwrap();

            for _ in 0 .. value.batch {
                if value.mode != Running {
                    return value.frame()
                }

                if let Some(frame) = value.cycle(breakpoints, hit_breakpoint) {
                    return frame
                }

                hit_breakpoint = false
            }
        }
    }
}

impl<Mem: Memory> Drop for Debugger<Mem> {
    fn drop(&mut self) {
        // Not enough, ARC will keep it alive!
        self.pause()
    }
}
