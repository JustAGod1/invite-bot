mod db;

#[macro_use]
extern crate log;


use teloxide::dptree;

use std::sync::Arc;
use log::LevelFilter;

use teloxide::{Bot, RequestError};
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::prelude::*;
use teloxide::types::ChatKind;
use teloxide::types::MessageKind;

use teloxide::utils::command::BotCommands;
use db::DBConn;
use std::io::Write;
use chrono::Local;
use env_logger::Builder;


const GROUP_ID: i64 = -1001509012802;
const OWNER_ID: u64 = 429171352;
const INVITE_LINK: &str = "https://t.me/+j-2EHIs0HqVhZjRi";

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

async fn run() -> Result<(), String> {
    info!("Starting...");
    let bot = Bot::from_env().auto_send();

    let dbb = Arc::new(DBConn::open()?);

    Dispatcher::builder(
        bot,
        Update::filter_message()
            .branch(
                dptree::filter(|msg: Message| {
                    if let MessageKind::Common(msg) = msg.kind {
                        msg.from.map(|from| from.id == UserId(OWNER_ID)).unwrap_or(false)
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
                    .endpoint(|m: Message, b: AutoSend<Bot>, db: Arc<DBConn>| async move {
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

async fn check_group_message(m: Message, b: AutoSend<Bot>, db: Arc<DBConn>) -> Result<(), String> {
    if let MessageKind::NewChatMembers(members) = m.kind {
        for member in members.new_chat_members {
            check_member(&member.id, b.clone(), db.clone()).await?
        }
    }
    Ok(())
}

async fn check_member(member: &UserId, b: AutoSend<Bot>, arc: Arc<DBConn>) -> Result<(), String> {
    let exists = arc.find_by_telegram_id(member.0)?.is_some();

    if !exists {
        b.kick_chat_member(ChatId(GROUP_ID), member.clone()).await.map_err(|a| a.to_string())?;
        b.unban_chat_member(ChatId(GROUP_ID), member.clone()).await.map_err(|a| a.to_string())?;
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
}

async fn answer(msg: Message, bot: AutoSend<Bot>, command: Command) -> Result<(), RequestError> {
    if !matches!(msg.chat.kind, ChatKind::Private(_)) {
        return Ok(());
    }
    info!("{:?}", msg);
    match command {
        Command::Help | Command::Start => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
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
    db: Arc<DBConn>,
) -> Result<(), RequestError> {
    let registered = if let MessageKind::Common(content) = msg.kind {
        match content.from.map(|from| db.find_by_telegram_id(from.id.0)).unwrap_or(Ok(None)) {
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
                     "Привет, и добро пожаловать на КТ!\n\
                        Прежде чем ты наконец познакомишься со своими однокурсниками, мне нужно добавить тебя в чат к ним.\n\
                        Для этого мне надо удостовериться, что ты есть в приказе на зачисление\n\
                        Пожалуйста пришли свое полное ФИО как оно написано на https://abit.itmo.ru/\n",
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
    db: Arc<DBConn>,
) -> Result<(), RequestError> {
    let format = "Пожалуйста отправь свое ФИО одним сообщением. Пример: Иванов Иван Иванович";
    let text = if let Some(text) = msg.text() {
        text.trim().to_string()
    } else {
        bot.send_message(msg.chat.id, format).await?;
        return Ok(());
    };

    let user = match db.find_by_full_name(&text) {
        Ok(v) => {
            if v.is_some() {
                bot.send_message(msg.chat.id, "Нашел! Секундочку...").await?;

                let user = v.as_ref().unwrap();
                let user_id = user.telegram_id.as_ref();

                if user_id.is_some() {
                    bot.send_message(msg.chat.id, format!("{}, кажется, ты уже зарегистрирован. На всякий случай вот тебе ссылка еще раз.\n\
                    {}\n\
                    Если произошла какая-то ошибка, пиши @JustAG0d", INVITE_LINK, user.full_name)).await?;

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

    if user.is_none() {
        bot.send_message(msg.chat.id, "Попробуй скопировать ФИО с https://abit.itmo.ru!").await?;
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
        if let Err(e) = db.insert_telegram_data(text, id.0) {
            error!("{}", e);
            bot.send_message(msg.chat.id, "Техническая ошибка.").await?;
            return Ok(());
        }

        bot.send_message(msg.chat.id, format!("Ура! Ты успешно прошел проверку! Теперь заходи скорей по ссылке: {}", INVITE_LINK)).await?;
    }

    return Ok(());
}

