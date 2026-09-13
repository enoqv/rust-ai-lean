pub fn f() -> String { itoa::Buffer::new().format(1).to_string() + &local_helper::name() }
