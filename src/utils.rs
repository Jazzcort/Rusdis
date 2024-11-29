use crate::command_parser::Command;
use rand::{distributions::Alphanumeric, Rng};
pub(crate) fn generate_random_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect::<String>()
}

pub(crate) fn generate_resp(cmd: Command) -> String {
    match cmd {
        Command::Set { key, value, px } => {
            let mut string = format!(
                "$3\r\nset\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
                key.len(),
                key,
                value.len(),
                value
            );

            let mut length = 3;

            if let Some(millis) = px {
                let millis_string = millis.to_string();
                string += format!(
                    "$2\r\npx\r\n${}\r\n{}\r\n",
                    millis_string.len(),
                    millis_string
                )
                .as_str();
                length += 2;
            }

            format!("*{}\r\n{}", length, string)
        }
        _ => {
            format!("$0\r\n\r\n")
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_resp() {
        let set_command = Command::Set {
            key: "key1".to_string(),
            value: "apple".to_string(),
            px: Some(5000),
        };
        assert_eq!(
            "*5\r\n$3\r\nset\r\n$4\r\nkey1\r\n$5\r\napple\r\n$2\r\npx\r\n$4\r\n5000\r\n",
            generate_resp(set_command)
        );
    }
}
