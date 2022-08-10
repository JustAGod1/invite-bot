#[macro_use]
extern crate log;


use teloxide::dptree;

use std::sync::{Arc, Mutex};
use log::LevelFilter;

use teloxide::{Bot, RequestError};
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::prelude::*;
use teloxide::types::{ChatKind, InputFile};
use teloxide::types::MessageKind;

use teloxide::utils::command::BotCommands;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use chrono::Local;
use env_logger::Builder;
use regex::Regex;
use orm::diesel::{EqAll, ExpressionMethods, QueryDsl, RunQueryDsl, update};
use orm::models::{NewUser, User};
use orm::schema::users::dsl::users;
use orm::schema::users::{full_name, telegram_id};


const GROUP_ID: i64 = -1001509012802;
const COMMANDER_IDS: &[UserId] = &[UserId(429171352), UserId(316671439), UserId(292062277), UserId(972295645)];
const INVITE_LINK: &str = "https://t.me/+5cOB3ZgEnVQyMzIy";

#[tokio::main]
async fn main() {
    Builder::new()
        .format(|buf, record| {
            writeln!(buf,
                     "{} [{}] ({}:{}) - {}",
                     Local::now().format("%Y-%m-%dT%H:%M:%S"),
                     record.level(),
                     record.file().unwrap_or(""),
                     record.line().unwrap_or(0),
                     record.args(),
            )
        })
        .filter(None, LevelFilter::Info)
        .init();
    loop {
        if let Err(e) = run().await {
            eprintln!("{}", e);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

type Database = Arc<Mutex<orm::diesel::PgConnection>>;

async fn run() -> Result<(), String> {
    info!("Starting...");
    let bot = Bot::from_env().auto_send();

    let dbb = Arc::new(Mutex::new(orm::establish_connection()?));
    info!("Connected to db");

    Dispatcher::builder(
        bot,
        Update::filter_message()
            .branch(
                dptree::filter(|msg: Message| {
                    if let MessageKind::Common(msg) = msg.kind {
                        msg.from.map(|from| COMMANDER_IDS.contains(&from.id)).unwrap_or(false)
                    } else {
                        false
                    }
                }).filter_command::<Command>().endpoint(answer)
            )
            .branch(
                dptree::filter(|msg: Message| {
                    matches!(msg.chat.kind, ChatKind::Private(_))
                }).branch(
                    Update::filter_message()
                        .enter_dialogue::<Message, InMemStorage<DialogState>, DialogState>()
                        .branch(dptree::case![DialogState::Start].endpoint(start_dialog))
                        .branch(dptree::case![DialogState::WaitingForName].endpoint(receive_name))
                )
            )
            .branch(
                dptree::filter(|msg: Message|
                    msg.chat.id.0 == GROUP_ID
                )
                    .endpoint(|m: Message, b: AutoSend<Bot>, db: Database| async move {
                        if let Err(e) = check_group_message(m, b, db.clone()).await {
                            error!("{}", e);
                        }

                        respond(())
                    })
            ),
    )
        .dependencies(dptree::deps![dbb.clone(), InMemStorage::<DialogState>::new()])
        .default_handler(|_| async {
            ()
        })
        .build()
        .setup_ctrlc_handler()
        .dispatch().await;

    Ok(())
}

async fn check_group_message(m: Message, b: AutoSend<Bot>, db: Database) -> Result<(), String> {
    if let MessageKind::NewChatMembers(members) = m.kind {
        for member in members.new_chat_members {
            check_member(&member.id, b.clone(), db.clone()).await?
        }
    }
    Ok(())
}

async fn check_member(member: &UserId, b: AutoSend<Bot>, arc: Database) -> Result<(), String> {
    use orm::schema::users;
    let r: Vec<orm::models::User> = users::table
        .filter(users::telegram_id.eq_all(member.0.to_string()))
        .load(&*arc.lock().unwrap())
        .map_err(|e| format!("{}", e))?;
    let exists = r.len() > 0;

    if !exists {
        b.kick_chat_member(ChatId(GROUP_ID), member.clone()).await.map_err(|a| a.to_string())?;
    }


    Ok(())
}


#[derive(BotCommands, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "display this text.")]
    Start,
    #[command(description = "<ФИО> - забывает ид пользователя с этим ФИО")]
    Forget,
    #[command(description = "отдает бд файлом")]
    Dump,
    #[command(description = "отдает бд сообщением")]
    DumpCsv,
    #[command(description = "<ФИО> - добавляет ФИО")]
    Add,
    #[command(description = "<ФИО> <id> - добавляет ФИО")]
    AddId,
    #[command(description = "резолвит ник")]
    Resolve,
}

fn update_telegram_id(db: Database, fullname: &str, id: &str) -> Result<bool, String> {
    update(users.filter(full_name.eq_all(fullname)))
        .set(telegram_id.eq_all(id.to_string()))
        .execute(&*db.lock().unwrap())
        .map_err(|a| a.to_string())
        .map(|a| a > 0)
}

fn find_by_telegram_id(db: Database, id: &str) -> Result<Option<User>, String> {
    users
        .filter(telegram_id.eq_all(id.to_string()))
        .limit(1)
        .load::<User>(&*db.lock().unwrap())
        .map(|a| a.into_iter().next())
        .map_err(|a| a.to_string())
}

fn find_by_fullname(db: Database, fullname: &str) -> Result<Option<User>, String> {
    users
        .filter(full_name.eq_all(fullname.to_string()))
        .limit(1)
        .load::<User>(&*db.lock().unwrap())
        .map(|a| a.into_iter().next())
        .map_err(|a| a.to_string())
}

async fn answer(msg: Message, bot: AutoSend<Bot>, command: Command, db: Database) -> Result<(), RequestError> {
    if !matches!(msg.chat.kind, ChatKind::Private(_)) {
        return Ok(());
    }
    info!("{:?}", msg);
    match command {
        Command::Help | Command::Start => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
        }

        Command::Forget => {
            let fullname = msg.text().unwrap_or("").trim()["/forget".len()..].trim();

            let r = orm::diesel::delete(orm::schema::users::table.filter(orm::schema::users::full_name.eq_all(fullname)))
                .execute(&*db.lock().unwrap());

            if let Err(e) = r {
                error!("{}", e);
                bot.send_message(msg.chat.id, format!("{}", e)).await?;
            } else {
                bot.send_message(msg.chat.id, format!("Забыл Telegram Id у {}", fullname)).await?;
            }
        }
        Command::Dump => {
            if let Err(e) = bot.send_document(msg.chat.id, InputFile::file(PathBuf::from("db.sqlite"))).await {
                error!("{:?}", e);
                bot.send_message(msg.chat.id, format!("{:?}", e)).await?;
            }
        }
        Command::Add => {
            let fullname = msg.text().unwrap_or("").trim()["/add".len()..].trim();

            let r = orm::diesel::insert_into(orm::schema::users::table)
                .values(NewUser {
                    full_name: fullname.to_string()
                })
                .execute(&*db.lock().unwrap());

            if let Err(e) = r {
                error!("{}", e);
                bot.send_message(msg.chat.id, format!("{}", e)).await?;
            } else {
                bot.send_message(msg.chat.id, format!("Добавил {}", fullname)).await?;
            }
        }
        Command::DumpCsv => {
            let u = users
                .order_by(orm::schema::users::full_name.asc())
                .load::<orm::models::User>(&*db.lock().unwrap());
            if let Err(e) = u {
                error!("{}", e);
                bot.send_message(msg.chat.id, format!("{}", e)).await?;
            } else {
                let mut v = String::new();
                let u = u.unwrap();

                for user in u {
                    v.push_str(format!("{},{:?},{:?}\n", user.full_name, user.phone, user.telegram_id).as_str())
                }

                let mut r = String::new();
                let mut i = 0;

                for line in v.split("\n") {
                    r.push_str(&line);
                    r.push('\n');
                    i += 1;

                    if i > 20 {
                        bot.send_message(msg.chat.id, &r).await?;
                        r.clear();
                        i = 0;
                    }
                }

                bot.send_message(msg.chat.id, v).await?;
            }
        }
        Command::AddId => {
            let fullname_and_id = msg.text().unwrap_or("").trim()["/addid".len()..].trim();
            if fullname_and_id.rfind(" ").is_none() {
                bot.send_message(msg.chat.id, "Неверный формат").await?;
                return Ok(());
            }
            let fullname = fullname_and_id[..fullname_and_id.rfind(" ").unwrap()].trim();
            let id = fullname_and_id[fullname_and_id.rfind(" ").unwrap()..].trim();


            let find_by_fullname = users
                .filter(full_name.eq_all(fullname))
                .load::<User>(&*db.lock().unwrap())
                .map(|a| a.len() > 0);

            match find_by_fullname {
                Ok(o) => {
                    if !o {
                        bot.send_message(msg.chat.id, "Нет такого ФИО").await?;
                        return Ok(());
                    } else {
                        if let Err(e) = update_telegram_id(db.clone(), fullname, id) {
                            error!("{}", e);
                            bot.send_message(msg.chat.id, format!("{}", e)).await?;
                        } else {
                            bot.send_message(msg.chat.id, format!("Обновил id у {} на {}", fullname, id)).await?;
                        }
                    }
                }
                Err(e) => {
                    error!("{}", e);
                    bot.send_message(msg.chat.id, format!("{}", e)).await?;
                }
            }
        }

        Command::Resolve => {
            let id = msg.text().unwrap_or("").trim()["/resolve".len()..].trim();
            match i64::from_str(id) {
                Ok(o) => {
                    let r = bot.get_chat(ChatId(o)).await?;
                    bot.send_message(msg.chat.id, format!("{}", r.username().unwrap_or("unknown"))).await?;
                }
                Err(e) => {
                    bot.send_message(msg.chat.id, format!("{}", e)).await?;
                }
            }
        }
    }

    return Ok(());
}

#[derive(Clone)]
enum DialogState {
    Start,
    WaitingForName,
}

impl Default for DialogState {
    fn default() -> Self {
        return DialogState::Start;
    }
}

async fn start_dialog(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: Dialogue<DialogState, InMemStorage<DialogState>>,
    db: Database,
) -> Result<(), RequestError> {
    let registered = if let MessageKind::Common(content) = msg.kind {
        match content.from.map(|from| find_by_telegram_id(db.clone(), &from.id.0.to_string())).unwrap_or(Ok(None)) {
            Ok(v) => { v }
            Err(e) => {
                error!("{}", e);
                bot.send_message(msg.chat.id, "Техническая ошибка.").await?;
                return Ok(());
            }
        }
    } else {
        None
    };

    if let Some(registered) = registered {
        bot.send_message(msg.chat.id, format!("{}, кажется, ты уже зарегистрирован. На всякий случай вот тебе ссылка еще раз.\n\
        {}\n\
        Если произошла какая-то ошибка, пиши @JustAG0d", registered.full_name, INVITE_LINK)).await?;
        return Ok(());
    }

    bot.send_message(msg.chat.id,
                     "Привет, и добро пожаловать на КТ!\n\n\
                           Для того, чтобы добавить тебя в чат и канал первокурсников, мне нужно удостовериться, что ты есть в приказе на зачисление.\n\
                           Пожалуйста, пришли свое полное ФИО и последние 4 цифры телефона как оно указано в личном кабинете абитуриента abitlk.itmo.ru\n\
                           Например: Иванов Иван Иванович 5411",
    ).send().await?;

    if let Err(e) = dialogue.update(DialogState::WaitingForName).await {
        error!("{:?}", e);
        bot.send_message(msg.chat.id, "Техническая ошибка.").await?;
        return Ok(());
    }

    Ok(())
}


async fn receive_name(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: Dialogue<DialogState, InMemStorage<DialogState>>,
    db: Database,
) -> Result<(), RequestError> {
    let format = "Пожалуйста отправь свое ФИО одним сообщением. Пример: Иванов Иван Иванович 5411";
    let text = if let Some(text) = msg.text() {
        Regex::new("\\s").unwrap().replace(text, " ").trim().to_string()
    } else {
        bot.send_message(msg.chat.id, format).await?;
        return Ok(());
    };
    if text.rfind(" ").is_none() {
        bot.send_message(msg.chat.id, format).await?;
        return Ok(());
    }

    let user = match find_by_fullname(db.clone(), &text[..text.rfind(' ').unwrap()]) {
        Ok(v) => {
            if v.is_some() {
                bot.send_message(msg.chat.id, "Нашел! Секундочку...").await?;

                let user = v.as_ref().unwrap();
                let user_id = user.telegram_id.as_ref();

                if user_id.is_some() {
                    bot.send_message(msg.chat.id, format!("{}, кажется, ты уже зарегистрирован. \n\
                    Если произошла какая-то ошибка, пиши @JustAG0d", user.full_name)).await?;

                    dialogue.update(DialogState::Start).await.unwrap();

                    return Ok(());
                }

                v
            } else {
                None
            }
        }
        Err(e) => {
            error!("{}", e);
            bot.send_message(msg.chat.id, "Техническая ошибка.").await?;
            return Ok(());
        }
    };


    let input_phone = &text[text.rfind(' ').unwrap() + 1..];

    let username = user.as_ref().map(|u| u.full_name.clone());
    let phone = user.as_ref().map(|u| u.phone.clone());

    if username.is_none() {
        bot.send_message(msg.chat.id, "Не нашел тебя среди зачисленных. Проверь, что ты скопировал ФИО и телефон из личного кабинета abitlk.itmo.ru").await?;
        dialogue.update(DialogState::WaitingForName).await.unwrap();
        return Ok(());
    }

    let phone = phone.unwrap();

    if user.as_ref().is_some() && user.as_ref().unwrap().phone.is_none() {
        bot.send_message(msg.chat.id, "К сожалению, мы не можем добавить тебя в чат автоматически. Напиши в лс @JustAG0d.").await?;
        dialogue.update(DialogState::WaitingForName).await.unwrap();
        return Ok(());
    }

    let phone = phone.unwrap();

    if &phone[phone.len() - 4..] != input_phone {
        bot.send_message(msg.chat.id, "Не нашел тебя среди зачисленных. Проверь, что ты скопировал ФИО и телефон из личного кабинета abitlk.itmo.ru").await?;
        dialogue.update(DialogState::WaitingForName).await.unwrap();
        return Ok(());
    }

    if let MessageKind::Common(content) = &msg.kind {
        let id = content.from.as_ref().map(|a| a.id);
        if id.is_none() {
            bot.send_message(msg.chat.id, "Почему то не могу прочитать отправителя сообщения.").await?;
            warn!("Cannot read sender id: {:?}", msg);
            return Ok(());
        }
        let id = id.unwrap();
        if let Err(e) = update_telegram_id(db.clone(), text[..text.rfind(' ').unwrap()].trim(), &id.0.to_string()) {
            error!("{}", e);
            bot.send_message(msg.chat.id, "Техническая ошибка.").await?;
            return Ok(());
        }

        bot.send_message(msg.chat.id, format!("Ура! Ты успешно прошел проверку! Обязательно подпишись и включи уведомления на новостной канал: @news_y2022, а также вступай в чат с однокурсниками и кураторами: {}", INVITE_LINK)).await?;
    }

    return Ok(());
}

