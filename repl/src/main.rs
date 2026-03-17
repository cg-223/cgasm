use std::error::Error;
use std::fs;
use std::io::stdin;

use compiler::run;

fn main() -> Result<(), Box<dyn Error>> {
    'main: loop {
        println!("enter lines until done... type an empty line when complete");
        let mut lines = String::new();
        loop {
            let mut strng = String::new();
            while let Err(err) = stdin().read_line(&mut strng) {
                eprintln!("{err}")
            }

            if strng.starts_with("file") {
                let mut inputs = Vec::new();

                let after = strng.trim_start_matches("file ");
                let mut args = after.split_whitespace();
                let Some(filename) = args.next() else {
                    println!("please supply a filename to file");
                    continue 'main;
                };

                for arg in args {
                    if let Ok(i) = arg.parse::<i64>() {
                        inputs.push(i);
                    } else {
                        for byte in arg.trim().as_bytes() {
                            inputs.push(*byte as i64)
                        }
                        inputs.push(0)
                    }
                }

                if let Ok(file) = fs::read_to_string(filename) {
                    run(file.as_str(), Some(inputs));
                } else if let Ok(file) = fs::read_to_string(format!("{filename}.cgasm")) {
                    run(file.as_str(), Some(inputs));
                } else {
                    println!("failed to open file: {filename}")
                }
                continue 'main;
            }

            if strng == "\r\n" {
                break;
            }

            lines.push_str(&strng);
        }

        run(&lines, None);
    }
}
