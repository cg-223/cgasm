use crate::{
    lexer::{Token, Unit},
    run,
};
use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{HashMap, VecDeque},
    ffi::CString,
    fs, i64,
    marker::PhantomData,
    str::FromStr,
};

static PARSER_PASSES: [fn(&mut ParseUnit); 4] = [
    find_labels,
    initial_parse_lines,
    convert_to_linear,
    validate_instrs,
];

fn validate_instrs(pu: &mut ParseUnit) {
    pu.validate_instrs();
}

fn convert_to_linear(pu: &mut ParseUnit) {
    let mut line_nums_to_instrs: HashMap<usize, (usize, Instruction)> = HashMap::new();

    let mut incrementer = 0;

    let mut lines = Vec::new();

    for (ln, line) in &pu.lines {
        lines.push((ln, line))
    }

    lines.sort_by_key(|x| x.0);

    for (ln, line) in lines {
        match line.to_single_instr() {
            Ok(Some(instr)) => {
                line_nums_to_instrs.insert(*ln, (incrementer, instr));
                incrementer += 1;
            }
            Ok(None) => (),
            Err(strng) => println!("{strng}"),
        }
    }

    let mut label_to_instr_point: HashMap<&str, usize> = HashMap::new();

    for (label, ln) in &pu.labels {
        if let Some((point, _)) = line_nums_to_instrs.get(ln) {
            if label.str == "start" {
                pu.start = *point as i64;
            }

            label_to_instr_point.insert(label.str, *point);
        } else {
            eprintln!("Failed to match label {label:?}")
        }
    }

    let mut instrs = Vec::new();
    for (_, (instr_point, instr)) in line_nums_to_instrs {
        instrs.push((instr_point, instr));
    }

    instrs.sort_by_key(|x| x.0);

    let mut instrs: Vec<Instruction> = instrs.drain(..).map(|x| x.1).collect();

    for (instr_point, instr) in instrs.iter_mut().enumerate() {
        for arg in &mut instr.args {
            if let InstrArg::Label(lbl_name) = arg
                && let Some(x) = label_to_instr_point.get(lbl_name.as_str())
            {
                *arg = InstrArg::Integer(*x as i64)
            }
        }
    }

    pu.late_linear = instrs;
}

#[derive(Debug)]
pub struct ParseUnit<'lu> {
    lexer_unit: &'lu Unit,
    lines: HashMap<usize, ParseLine<'lu>>,
    labels: HashMap<Label<'lu>, usize>,

    start: i64,
    late_linear: Vec<Instruction>,
    inputs: VecDeque<i64>,
}

#[derive(Debug)]
enum LateInstruction {
    Early(Instruction),
    Label(usize),
}

fn find_labels(pu: &mut ParseUnit) {
    for (mut line, line_num) in pu
        .lexer_unit
        .lines()
        .iter()
        .enumerate()
        .map(|x| (x.1.tokens().iter(), x.0))
    {
        if let Some(Token::Label(label_name)) = line.next() {
            let (None | Some(Token::EOF)) = line.next() else {
                eprintln!("malformed label: {line:?}");
                continue;
            };

            pu.labels.insert(Label { str: label_name }, line_num + 1);
        }
    }
}

fn initial_parse_lines(pu: &mut ParseUnit) {
    for (i, line) in pu.lexer_unit.lines().iter().enumerate() {
        let parsed = LineWalker::new(line.tokens(), line.src()).parse();
        match parsed.value {
            ParseLineType::Error(e) => eprintln!("{e}"),
            ParseLineType::Empty => (),
            ParseLineType::Instr(_) => {
                pu.lines.insert(i, parsed);
            }
        }
    }
}

#[derive(Debug)]
struct LineWalker<'lu> {
    src: &'lu str,
    toks: &'lu [Token],
    pos: usize,
}

impl<'lu> LineWalker<'lu> {
    fn all_toks(&self) -> &[Token] {
        self.toks
    }

    fn next(&mut self) -> Option<&Token> {
        self.pos += 1;
        self.toks.get(self.pos - 1)
    }

    fn parse(&mut self) -> ParseLine<'lu> {
        match self.next() {
            None => ParseLine::empty(),
            Some(Token::Ident(ident)) => {
                let instr = match ident.as_str() {
                    "add" => Instruction::parse_with_nargs(self, 2, InstrType::Add),
                    "sub" => Instruction::parse_with_nargs(self, 2, InstrType::Sub),
                    "mul" => Instruction::parse_with_nargs(self, 2, InstrType::Mul),
                    "div" => Instruction::parse_with_nargs(self, 2, InstrType::Div),
                    "jif" => Instruction::parse_with_nargs(self, 2, InstrType::Jif),
                    "set" => Instruction::parse_with_nargs(self, 2, InstrType::Set),
                    "cmp" => Instruction::parse_with_nargs(self, 2, InstrType::Cmp),
                    "eq" => Instruction::parse_with_nargs(self, 2, InstrType::Eq),
                    "setif" => Instruction::parse_with_nargs(self, 3, InstrType::Setif),
                    "print" => Instruction::parse_with_nargs(self, 1, InstrType::Print),
                    "call" => Instruction::parse_with_nargs(self, 1, InstrType::Call),
                    "ret" => Instruction::parse_with_nargs(self, 0, InstrType::Ret),
                    "nop" => Instruction::parse_with_nargs(self, 0, InstrType::Nop),
                    "input" => Instruction::parse_with_nargs(self, 1, InstrType::Input),
                    "inputstr" => Instruction::parse_with_varargs(self, InstrType::InputStr),
                    "printstr" => Instruction::parse_with_nargs(self, 1, InstrType::PrintStr),
                    "jmp" => Instruction::parse_with_nargs(self, 1, InstrType::Jmp),
                    "deref" => Instruction::parse_with_nargs(self, 1, InstrType::Deref),
                    "retif" => Instruction::parse_with_nargs(self, 1, InstrType::Retif),
                    "pop" => Instruction::parse_with_varargs(self, InstrType::Pop),
                    "push" => Instruction::parse_with_nargs(self, 1, InstrType::Push),
                    "readstack" => Instruction::parse_with_nargs(self, 2, InstrType::ReadStack),
                    "writestack" => Instruction::parse_with_nargs(self, 1, InstrType::WriteStack),
                    "callif" => Instruction::parse_with_nargs(self, 2, InstrType::CallIf),
                    "hasinput" => Instruction::parse_with_nargs(self, 1, InstrType::HasInput),
                    "file" => Instruction::parse_with_varargs(self, InstrType::File),
                    _ => {
                        return ParseLine::error(format!("invalid instruction: {ident}"), self.src);
                    }
                };

                match instr {
                    Ok(instr) => ParseLine {
                        value: ParseLineType::Instr(instr),
                        line: self.src,
                    },
                    Err(e) => ParseLine::error(e, self.src),
                }
            }
            Some(Token::Label(_)) | Some(Token::None) => ParseLine::empty(),
            Some(tok) => ParseLine::error(format!("invalid start of line: {tok:?}"), self.src),
        }
    }

    fn new(toks: &'lu [Token], src: &'lu str) -> Self {
        Self { toks, pos: 0, src }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Clone, Copy)]
pub struct Label<'lu> {
    str: &'lu str,
}

impl<'lu> Label<'lu> {
    fn get_line(&self, unit: &'lu ParseUnit) -> Option<&ParseLine<'lu>> {
        unit.lines.get(unit.labels.get(self)?)
    }
}

impl<'lu> ParseUnit<'lu> {
    pub fn parse(unit: &'lu Unit) -> Self {
        let mut slf = Self {
            lines: HashMap::new(),
            labels: HashMap::new(),
            lexer_unit: unit,
            late_linear: Vec::new(),
            start: 0,
            inputs: VecDeque::new(),
        };

        for pass in PARSER_PASSES {
            pass(&mut slf);
        }

        slf
    }

    pub fn add_inputs(&mut self, inputs: Vec<i64>) {
        for input in inputs {
            self.inputs.push_front(input);
        }
    }

    pub fn late_instrs(&self) -> &[Instruction] {
        &self.late_linear
    }

    pub fn validate_instrs(&self) {
        for instr in self.late_instrs() {
            let mut args = instr.args.iter();
            use InstrArg as IA;
            use InstrType as IT;
            match instr.ty {
                IT::Nop | IT::Ret => assert!(
                    args.next().is_none(),
                    "failed to match for instruction {instr:?}"
                ),
                IT::Add
                | IT::Sub
                | IT::Mul
                | IT::Div
                | IT::Set
                | IT::Setif
                | IT::Input
                | IT::InputStr
                | IT::PrintStr
                | IT::Deref
                | IT::Pop
                | IT::ReadStack
                | IT::WriteStack
                | IT::HasInput => assert!(
                    matches!(args.next().unwrap(), IA::Memory(_) | IA::DoubleMemory(_)),
                    "failed to match for instruction {instr:?}"
                ),
                IT::Retif
                | IT::Call
                | IT::CallIf
                | IT::Eq
                | IT::Cmp
                | IT::Jmp
                | IT::Jif
                | IT::Push => {
                    assert!(matches!(
                        args.next().unwrap(),
                        IA::Integer(_) | IA::Memory(_) | IA::DoubleMemory(_)
                    ))
                }
                IT::Print => {
                    assert!(
                        matches!(args.next().unwrap(), _),
                        "failed to match for instruction {instr:?}"
                    )
                }
                IT::File => assert!(
                    matches!(
                        args.next().unwrap(),
                        IA::DoubleMemory(_) | IA::Memory(_) | IA::String(_)
                    ),
                    "failed to match for instruction {instr:?}"
                ),
            }

            match instr.ty {
                IT::Nop
                | IT::Ret
                | IT::Deref
                | IT::Push
                | IT::Jmp
                | IT::Call
                | IT::Print
                | IT::Input
                | IT::Pop
                | IT::Retif
                | IT::HasInput => {
                    assert!(
                        args.next().is_none(),
                        "failed to match for instruction {instr:?}"
                    )
                }
                IT::Add
                | IT::Sub
                | IT::Mul
                | IT::Div
                | IT::ReadStack
                | IT::Eq
                | IT::Cmp
                | IT::WriteStack => assert!(
                    matches!(
                        args.next().unwrap(),
                        IA::Integer(_) | IA::Memory(_) | IA::DoubleMemory(_)
                    ),
                    "failed to match for instruction {instr:?}"
                ),
                IT::Set | IT::Setif => assert!(
                    matches!(
                        args.next().unwrap(),
                        IA::Integer(_) | IA::Memory(_) | IA::DoubleMemory(_) | IA::String(_)
                    ),
                    "failed to match for instruction {instr:?}"
                ),
                IT::PrintStr | IT::InputStr => assert!(
                    matches!(
                        args.next(),
                        None | Some(IA::Integer(_))
                            | Some(IA::Memory(_))
                            | Some(IA::DoubleMemory(_))
                    ),
                    "failed to match for instruction {instr:?}"
                ),
                IT::CallIf | IT::Jif => {
                    assert!(
                        matches!(args.next().unwrap(), IA::Memory(_) | IA::DoubleMemory(_)),
                        "failed to match for instruction {instr:?}"
                    )
                }
                IT::File => assert!(
                    matches!(
                        args.next(),
                        None | Some(IA::Memory(_))
                            | Some(IA::DoubleMemory(_))
                            | Some(IA::Integer(_))
                            | Some(IA::String(_))
                    ),
                    "failed to match for instruction {instr:?}"
                ),
            }

            match instr.ty {
                IT::Add
                | IT::Sub
                | IT::Mul
                | IT::Div
                | IT::Set
                | IT::Nop
                | IT::Ret
                | IT::Push
                | IT::Jmp
                | IT::Call
                | IT::Print
                | IT::Input
                | IT::InputStr
                | IT::PrintStr
                | IT::Deref
                | IT::Pop
                | IT::Eq
                | IT::Cmp
                | IT::ReadStack
                | IT::Retif
                | IT::CallIf
                | IT::Jif
                | IT::WriteStack
                | IT::HasInput => {
                    assert!(
                        args.next().is_none(),
                        "failed to match for instruction {instr:?}"
                    )
                }

                IT::Setif => {
                    assert!(
                        matches!(args.next().unwrap(), IA::Memory(_) | IA::DoubleMemory(_)),
                        "failed to match for instruction {instr:?}"
                    )
                }
                IT::File => assert!(
                    matches!(
                        args.next(),
                        None | Some(IA::Memory(_))
                            | Some(IA::DoubleMemory(_))
                            | Some(IA::Integer(_))
                    ),
                    "failed to match for instruction {instr:?}"
                ),
            }
        }
    }

    pub fn better_execute(&self) {
        let mut memory: RefCell<HashMap<i64, i64>> = RefCell::new(HashMap::new());
        let mut ret_addr_stack: Vec<i64> = Vec::new();
        let mut stack: Vec<i64> = Vec::new();
        let mut inputs = self.inputs.clone();

        let arg_to_memory = |arg: &InstrArg| -> i64 {
            match arg {
                InstrArg::Memory(mem) => *mem,
                InstrArg::DoubleMemory(mem) => *memory.borrow().get(mem).unwrap_or(&0),
                _ => panic!(),
            }
        };
        let arg_to_integer = |arg: &InstrArg| -> i64 {
            match arg {
                InstrArg::Memory(mem) => *memory.borrow().get(mem).unwrap_or(&0),
                InstrArg::DoubleMemory(mem) => *memory
                    .borrow()
                    .get(memory.borrow().get(mem).unwrap_or(&0))
                    .unwrap_or(&0),
                InstrArg::Integer(i) => *i,
                _ => panic!(),
            }
        };

        let get_arg_to_integer = |arg: &InstrArg| -> Option<i64> {
            match arg {
                InstrArg::Memory(mem) => memory.borrow().get(mem).cloned(),
                InstrArg::Integer(i) => Some(*i),
                InstrArg::None => None,
                _ => panic!(),
            }
        };

        let modify_in_place = |memarg, arg, f: fn(i64, i64) -> i64| {
            let at_place = arg_to_integer(memarg);
            let at_arg = arg_to_integer(arg);
            let t = arg_to_memory(memarg);

            memory.borrow_mut().insert(t, f(at_place, at_arg));
        };

        let mut ip: i64 = self.start;

        while let Some(instr) = self.late_linear.get(ip as usize) {
            #[cfg(debug_assertions)]
            println!("executing instr {instr:?}");
            match instr.ty {
                InstrType::Nop => {}
                InstrType::Print => match &instr.args[0] {
                    InstrArg::Float(flt) => println!("{flt}"),
                    InstrArg::Integer(int) => println!("{int}"),
                    InstrArg::Memory(memaddr) => {
                        println!("{}", memory.borrow_mut().entry(*memaddr).or_insert(0))
                    }
                    InstrArg::DoubleMemory(memaddr) => {
                        println!(
                            "{}",
                            memory
                                .borrow()
                                .get(memory.borrow().get(memaddr).unwrap_or(&0))
                                .unwrap_or(&0)
                        )
                    }
                    InstrArg::Label(lbl) => println!("'{lbl}"),
                    InstrArg::String(str) => println!("{}", str.to_string_lossy()),
                    InstrArg::None => println!("None"),
                },
                InstrType::PrintStr => {
                    let mut memloc = arg_to_memory(&instr.args[0]);

                    let mut strng = String::new();
                    loop {
                        let x = *memory.borrow().get(&memloc).unwrap_or(&0) as u8 as char;
                        if x as u8 == 0 {
                            break;
                        }
                        memloc += 1;
                        strng.push(x);
                    }

                    println!("{strng}")
                }
                InstrType::Add => {
                    modify_in_place(&instr.args[0], &instr.args[1], |x, y| x + y);
                }
                InstrType::Sub => {
                    modify_in_place(&instr.args[0], &instr.args[1], |x, y| x - y);
                }
                InstrType::Div => {
                    modify_in_place(&instr.args[0], &instr.args[1], |x, y| x / y);
                }
                InstrType::Mul => {
                    modify_in_place(&instr.args[0], &instr.args[1], |x, y| x * y);
                }
                InstrType::Set => {
                    if matches!(instr.args[1], InstrArg::Integer(_) | InstrArg::Memory(_)) {
                        modify_in_place(&instr.args[0], &instr.args[1], |_, y| y);
                    } else if let InstrArg::String(cstr) = &instr.args[1] {
                        let mut memloc = arg_to_memory(&instr.args[0]);
                        for byte in cstr.as_bytes() {
                            memory.borrow_mut().insert(memloc, *byte as i64);
                            memloc += 1;
                        }
                    }
                }
                InstrType::Jif => {
                    let value_at_first_arg = arg_to_integer(&instr.args[1]);

                    let target = arg_to_integer(&instr.args[0]);

                    if value_at_first_arg != 0 {
                        ip = target - 1;
                    }
                }
                InstrType::Setif => {
                    let addr_target = arg_to_memory(&instr.args[0]);

                    let conditional = arg_to_integer(&instr.args[2]);

                    if conditional != 0 {
                        let to_set = arg_to_integer(&instr.args[1]);

                        memory.borrow_mut().insert(addr_target, to_set);
                    }
                }
                InstrType::Call => {
                    ret_addr_stack.push(ip);

                    let target = arg_to_integer(&instr.args[0]);

                    ip = target - 1;
                }
                InstrType::CallIf => {
                    if arg_to_integer(&instr.args[1]) != 0 {
                        ret_addr_stack.push(ip);

                        let target = arg_to_integer(&instr.args[0]);

                        ip = target - 1;
                    }
                }
                InstrType::Ret => {
                    if let Some(new_ip) = ret_addr_stack.pop() {
                        ip = new_ip;
                    } else {
                        ip = i64::MAX - 16;
                    }
                }
                InstrType::Retif => {
                    if arg_to_integer(&instr.args[0]) != 0 {
                        if let Some(new_ip) = ret_addr_stack.pop() {
                            ip = new_ip;
                        } else {
                            ip = i64::MAX - 16;
                        }
                    }
                }
                InstrType::Cmp => {
                    let first = arg_to_integer(&instr.args[0]);
                    let second = arg_to_integer(&instr.args[1]);

                    memory.borrow_mut().insert(
                        -1,
                        match first.cmp(&second) {
                            Ordering::Equal => 0,
                            Ordering::Greater => 1,
                            Ordering::Less => -1,
                        },
                    );
                }
                InstrType::Eq => {
                    let first = arg_to_integer(&instr.args[0]);
                    let second = arg_to_integer(&instr.args[1]);

                    memory.borrow_mut().insert(
                        -1,
                        match first.eq(&second) {
                            true => 1,
                            false => 0,
                        },
                    );
                }
                InstrType::Jmp => {
                    let target = arg_to_integer(&instr.args[0]);

                    ip = target - 1;
                }
                InstrType::Input => {
                    let memloc = arg_to_memory(&instr.args[0]);

                    if let Some(input) = inputs.pop_back() {
                        memory.borrow_mut().insert(memloc, input);
                    } else {
                        let input = std::io::stdin();
                        let mut strng = String::new();

                        input.read_line(&mut strng);

                        memory
                            .borrow_mut()
                            .insert(memloc, strng.trim().parse().unwrap());
                    }
                }
                InstrType::Deref => {
                    let memloc = arg_to_memory(&instr.args[0]);
                    let i = *memory
                        .borrow()
                        .get(&arg_to_integer(&instr.args[0]))
                        .unwrap_or(&0);
                    memory.borrow_mut().insert(memloc, i);
                }
                InstrType::InputStr => {
                    let mut memloc = arg_to_memory(&instr.args[0]);
                    let origloc = memloc;
                    let maxn = get_arg_to_integer(instr.args.get(1).unwrap_or(&InstrArg::None))
                        .unwrap_or(i64::MAX);

                    if inputs.contains(&0) {
                        while let Some(input) = inputs.pop_back() {
                            if memloc - origloc >= maxn {
                                memory.borrow_mut().insert(memloc, 0);
                                break;
                            }

                            memory.borrow_mut().insert(memloc, input);
                            memloc += 1;

                            if input == 0 {
                                break;
                            }
                        }
                    } else {
                        let input = std::io::stdin();
                        let mut strng = String::new();

                        input.read_line(&mut strng);
                        let strng = strng.trim();
                        for byte in strng.as_bytes() {
                            if memloc - origloc >= maxn {
                                break;
                            }
                            memory.borrow_mut().insert(memloc, *byte as i64);
                            memloc += 1;
                        }

                        memory.borrow_mut().insert(memloc, 0);
                    }
                }
                InstrType::Pop => {
                    let off_stack = stack.pop().unwrap_or(0);
                    let memaddr = arg_to_memory(&instr.args[0]);
                    memory.borrow_mut().insert(memaddr, off_stack);
                }
                InstrType::Push => {
                    let item = arg_to_integer(&instr.args[0]);
                    stack.push(item);
                }
                InstrType::ReadStack => {
                    let offset = arg_to_integer(&instr.args[0]);
                    let memaddr = arg_to_memory(&instr.args[1]);

                    let stack_item = stack.get(stack.len() - offset as usize).unwrap_or(&0);

                    memory.borrow_mut().insert(memaddr, *stack_item);
                }
                InstrType::WriteStack => {
                    let offset = arg_to_integer(&instr.args[0]);
                    let item = arg_to_integer(&instr.args[1]);

                    let target = stack.len() - offset as usize;
                    stack[target] = item;
                }
                InstrType::HasInput => {
                    let memaddr = arg_to_integer(&instr.args[0]);
                    if !inputs.is_empty() {
                        memory.borrow_mut().insert(memaddr, 1);
                    } else {
                        memory.borrow_mut().insert(memaddr, 0);
                    }
                }
                InstrType::File => {
                    let mut string = String::new();

                    match &instr.args[0] {
                        InstrArg::DoubleMemory(_) | InstrArg::Memory(_) => {
                            let mut memloc = arg_to_memory(&instr.args[0]);

                            loop {
                                let x = *memory.borrow().get(&memloc).unwrap_or(&0) as u8 as char;
                                if x as u8 == 0 {
                                    break;
                                }
                                memloc += 1;
                                string.push(x);
                            }
                        }
                        InstrArg::String(strng) => {
                            string = strng.to_string_lossy().to_string();
                        }
                        _ => panic!(),
                    }

                    let mut args = Vec::new();

                    if let Some(addr) = match &instr.args.get(1) {
                        Some(InstrArg::DoubleMemory(memaddr)) => {
                            Some(*memory.borrow().get(memaddr).unwrap_or(&0))
                        }
                        Some(InstrArg::Memory(memaddr)) => Some(*memaddr),
                        Some(InstrArg::String(_)) => None,
                        Some(_) => panic!(),
                        None => None,
                    } {
                        let max_bytes = if let Some(arg) = &instr.args.get(2) {
                            arg_to_integer(arg)
                        } else {
                            i64::MAX
                        };

                        let mut bytes_read = 0;
                        loop {
                            if bytes_read >= max_bytes {
                                break;
                            }
                            let x = memory
                                .borrow()
                                .get(&(addr + bytes_read))
                                .cloned()
                                .unwrap_or(0);
                            args.push(x);
                            if x == 0 && max_bytes == i64::MAX {
                                break;
                            }
                            bytes_read += 1;
                        }
                    }

                    if let Some(InstrArg::String(strng)) = &instr.args.get(1) {
                        let max_bytes = if let Some(arg) = instr.args.get(2) {
                            arg_to_integer(arg)
                        } else {
                            i64::MAX
                        };

                        let bytes = strng.as_bytes();

                        for i in 0..max_bytes.min(bytes.len() as i64) {
                            args.push(bytes[i as usize] as i64)
                        }
                    }

                    let file = fs::read_to_string(string);

                    let exit_code = match file {
                        Ok(file) => {
                            run(&file, Some(args));
                            0
                        }

                        Err(ferr) => ferr.raw_os_error().map(|x| x as i64).unwrap_or(i64::MIN),
                    };

                    memory.borrow_mut().insert(0, exit_code);
                }
            }
            ip += 1;
        }
    }
}

#[derive(Debug)]
struct ParseLine<'lu> {
    value: ParseLineType,
    line: &'lu str,
}

static EMPTY_STR: &str = "";

impl<'lu> ParseLine<'lu> {
    fn error(err_msg: String, line: &'lu str) -> Self {
        Self {
            line,
            value: ParseLineType::Error(err_msg),
        }
    }

    fn empty() -> Self {
        Self {
            line: EMPTY_STR,
            value: ParseLineType::Empty,
        }
    }

    fn to_single_instr(&self) -> Result<Option<Instruction>, &str> {
        match &self.value {
            ParseLineType::Error(emsg) => Err(emsg),
            ParseLineType::Instr(instr) => Ok(Some(instr.clone())),
            ParseLineType::Empty => Ok(None),
        }
    }
}

#[derive(Debug)]
enum ParseLineType {
    Instr(Instruction),
    Error(String),
    Empty,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    ty: InstrType,
    args: Vec<InstrArg>,
}

impl Instruction {
    fn parse_with_varargs(line: &mut LineWalker, ty: InstrType) -> Result<Self, String> {
        let mut args = Vec::new();

        loop {
            args.push(match line.next() {
                None => break,
                Some(Token::Integer(int)) => InstrArg::Integer(*int),
                Some(Token::Float(flt)) => InstrArg::Float(*flt),
                Some(Token::String(strng)) => InstrArg::String(CString::from_str(strng).unwrap()),
                Some(Token::Memory(mem)) if *mem >= 0 => InstrArg::Memory(*mem),
                Some(Token::Label(lbl)) => InstrArg::Label(lbl.clone()),
                Some(Token::DoubleMemory(dbl)) => InstrArg::DoubleMemory(*dbl),
                Some(other) => return Err(format!("invalid argument to instruction: {other:?}")),
            });
        }

        Ok(Self { ty, args })
    }

    fn parse_with_nargs(
        line: &mut LineWalker,
        nargs: usize,
        ty: InstrType,
    ) -> Result<Self, String> {
        let mut args = Vec::new();

        for i in 0..nargs {
            args.push(match line.next() {
                None => {
                    return Err(format!(
                        "expected {nargs} args to instruction {ty:?}, found only {i}"
                    ));
                }
                Some(Token::Integer(int)) => InstrArg::Integer(*int),
                Some(Token::Float(flt)) => InstrArg::Float(*flt),
                Some(Token::Memory(mem)) => InstrArg::Memory(*mem),
                Some(Token::Label(lbl)) => InstrArg::Label(lbl.clone()),
                Some(Token::Ident(ident)) if ident.len() == 1 => {
                    InstrArg::Integer(ident.as_bytes()[0] as i64)
                }
                Some(Token::String(strng)) => {
                    InstrArg::String(CString::new(strng.as_bytes()).unwrap())
                }
                Some(Token::DoubleMemory(dbl)) => InstrArg::DoubleMemory(*dbl),
                Some(other) => return Err(format!("invalid argument to instruction: {other:?}")),
            });
        }

        if let Some(tok) = line.next() {
            return Err(format!(
                "Too many arguments to instruction {ty:?}, expected {nargs} but found {}",
                line.toks.len() - 1
            ));
        }

        Ok(Self { ty, args })
    }
}

#[derive(Debug, Clone)]
enum InstrArg {
    DoubleMemory(i64),
    Memory(i64),
    Integer(i64),
    Float(f64),
    Label(String),
    String(CString),
    None,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum InstrType {
    Nop,
    Add,
    Sub,
    Mul,
    Div,
    Set,
    Jif,
    Print,
    Setif,
    Call,
    Ret,
    Cmp,
    Input,
    InputStr,
    Jmp,
    PrintStr,
    Deref,
    Eq,
    Retif,
    Push,
    Pop,
    ReadStack,
    WriteStack,
    CallIf,
    HasInput,
    File,
}
