use std::{thread, time::Duration};

use async_std::task::sleep;
use nblock::{
    asynch::AsyncTask,
    task::{Nonblock, Task},
};

fn main() {
    println!("RUNNING!");
    let fut = async move {
        sleep(Duration::from_secs(1)).await;
        "hello, world!".to_owned()
    };
    let mut task = AsyncTask::from(fut);
    loop {
        match task.drive() {
            Nonblock::Idle => {
                thread::sleep(Duration::from_millis(10));
            }
            Nonblock::Active => println!("ACTIVE!"),
            Nonblock::Complete(x) => {
                println!("COMPLETE: {x}");
                break;
            }
        }
    }
}
