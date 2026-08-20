use chrono::{DateTime, Utc};
use discord_webhook_lib::DiscordMessage;
use figment2::{
    Figment,
    providers::{Format, Toml},
};
use futures::stream::StreamExt;
use log::{info, warn};
use ohno::AppError;
use poise::{
    Framework, FrameworkContext, FrameworkOptions,
    serenity_prelude::{
        CacheHttp, ClientBuilder, Context as SerenityContext, FullEvent, GatewayIntents, GuildId,
        OnlineStatus, PrimaryGuild, RoleId, ShardManager, User, UserId,
    },
};
use serde::{Deserialize, Serialize};
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use signal_hook_tokio::Signals;
use sqlx::query;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::sync::Arc;
use tabular::{Table, row};

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Config {
    token_file: String,
    log_channel_webhook_file: String,
    mod_role: Option<u64>,
    main_server: u64,
    qurantine_add_role: Option<u64>,
    qurantine_prevent_role: Option<u64>,
}

#[derive(Clone, Debug)]
struct Database {
    db: SqlitePool,
}

impl Database {
    pub async fn migrate(&self) -> Result<(), AppError> {
        sqlx::migrate!().run(&self.db).await?;
        return Ok(());
    }

    pub async fn count_records(&self) -> Result<(i64, i64, i64, i64), AppError> {
        let banned_tags = query!("SELECT COUNT(*) as count FROM banned_tags")
            .map(|rec| rec.count)
            .fetch_one(&self.db)
            .await?;
        let exempted_users = query!("SELECT COUNT(*) as count FROM exempted_users")
            .map(|rec| rec.count)
            .fetch_one(&self.db)
            .await?;
        let banned_users = query!("SELECT COUNT(*) as count FROM banned_users")
            .map(|rec| rec.count)
            .fetch_one(&self.db)
            .await?;
        let actions = query!("SELECT COUNT(*) as count FROM actions")
            .map(|rec| rec.count)
            .fetch_one(&self.db)
            .await?;
        Ok((banned_tags, exempted_users, banned_users, actions))
    }

    pub async fn get_all_exempt_users(&self) -> Result<Vec<(UserId, DateTime<Utc>)>, AppError> {
        let users = query!("SELECT * FROM exempted_users")
            .map(|rec| {
                let user_id = UserId::new(rec.user_id as u64);
                let time = DateTime::<Utc>::from_timestamp(rec.timestamp, 0)
                    .unwrap_or(DateTime::<Utc>::default());
                (user_id, time)
            })
            .fetch_all(&self.db)
            .await?;
        Ok(users)
    }

    pub async fn is_user_exempted(&self, user: UserId) -> Result<bool, AppError> {
        let user_id = user.get() as i64;
        let exists = query!(
            "SELECT * FROM exempted_users WHERE user_id = $1 LIMIT 1",
            user_id
        )
        .fetch_optional(&self.db)
        .await?
        .is_some();
        Ok(exists)
    }

    pub async fn delete_exempted_user(&self, user: UserId) -> Result<(), AppError> {
        let user_id = user.get() as i64;
        let _ = query!("DELETE FROM exempted_users WHERE user_id = $1", user_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn insert_exempt_user(&self, user: UserId) -> Result<(), AppError> {
        let user_id = user.get() as i64;
        let now = chrono::Utc::now().timestamp();
        let _ = query!("INSERT INTO exempted_users VALUES ($1, $2)", user_id, now)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    // pub async fn get_banned_tag(
    //     &self,
    //     guild_id: GuildId,
    // ) -> Result<Option<(Option<GuildId>, Option<String>)>, AppError> {
    //     let guild_id = guild_id.get() as i64;
    //     let banned = query!(
    //         "SELECT * FROM banned_tags WHERE server_id = $1 LIMIT 1",
    //         guild_id
    //     )
    //     .map(|rec| (rec.server_id.map(|x| GuildId::new(x as u64)), rec.tag))
    //     .fetch_optional(&self.db)
    //     .await?;
    //     Ok(banned)
    // }

    pub async fn get_all_banned_tags(
        &self,
    ) -> Result<Vec<(i64, Option<GuildId>, Option<String>)>, AppError> {
        let banned = query!("SELECT rowid, * FROM banned_tags")
            .map(|rec| {
                (
                    rec.rowid,
                    rec.server_id.map(|x| GuildId::new(x as u64)),
                    rec.tag,
                )
            })
            .fetch_all(&self.db)
            .await?;
        Ok(banned)
    }

    pub async fn is_tag_banned_by_guild_id(&self, guild_id: GuildId) -> Result<bool, AppError> {
        let guild_id = guild_id.get() as i64;
        let exists = query!(
            "SELECT * FROM banned_tags WHERE server_id = $1 LIMIT 1",
            guild_id
        )
        .fetch_optional(&self.db)
        .await?
        .is_some();
        Ok(exists)
    }

    pub async fn is_tag_banned_by_tag(&self, tag: &str) -> Result<bool, AppError> {
        let exists = query!("SELECT * FROM banned_tags WHERE tag = $1 LIMIT 1", tag)
            .fetch_optional(&self.db)
            .await?
            .is_some();
        Ok(exists)
    }

    pub async fn insert_banned_tag(
        &self,
        guild_id: Option<GuildId>,
        tag_text: Option<String>,
    ) -> Result<(), AppError> {
        let guild_id = guild_id.map(|x| x.get() as i64);
        let _ = query!(
            "INSERT INTO banned_tags VALUES ($1, $2)",
            guild_id,
            tag_text,
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn delete_banned_tag(&self, id: i64) -> Result<(), AppError> {
        let _ = query!("DELETE FROM banned_tags WHERE rowid = $1", id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn insert_action(&self, action_done: &str, user: UserId) -> Result<(), AppError> {
        let user_id = user.get() as i64;
        let now = chrono::Utc::now().timestamp();
        let _ = query!(
            "INSERT INTO actions VALUES ($1, $2, $3)",
            action_done,
            user_id,
            now
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn is_user_banned(&self, user: UserId) -> Result<bool, AppError> {
        let user_id = user.get() as i64;
        let exists = query!(
            "SELECT * FROM banned_users WHERE user_id = $1 LIMIT 1",
            user_id
        )
        .fetch_optional(&self.db)
        .await?
        .is_some();
        Ok(exists)
    }

    pub async fn get_all_banned_users(&self) -> Result<Vec<(i64, UserId)>, AppError> {
        let banned = query!("SELECT rowid, * FROM banned_users")
            .map(|rec| (rec.rowid, UserId::new(rec.user_id as u64)))
            .fetch_all(&self.db)
            .await?;
        Ok(banned)
    }

    pub async fn ban_user(&self, user: UserId) -> Result<(), AppError> {
        let user_id = user.get() as i64;
        let _ = query!("INSERT INTO banned_users VALUES ($1)", user_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn unban_user(&self, user: UserId) -> Result<(), AppError> {
        let user_id = user.get() as i64;
        let _ = query!("DELETE FROM banned_users WHERE user_id = $1", user_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), AppError> {
        self.db.close().await;
        Ok(())
    }
}

struct Data {
    config: Config,
    db: Database,
    webhook: String,
}

type Ctx<'a> = poise::Context<'a, Arc<Data>, AppError>;

async fn check_permissions(ctx: Ctx<'_>) -> Result<bool, AppError> {
    let data = ctx.data();
    if let Some(member) = ctx.author_member().await {
        if let Some(permissions) = &member.permissions {
            if permissions.administrator() {
                return Ok(true);
            }
        }
        if let Some(modroleid) = data.config.mod_role {
            let mod_role = RoleId::new(modroleid);
            return Ok(member.roles.contains(&mod_role));
        }
    }
    Ok(false)
}

async fn event_handler(
    framework: FrameworkContext<'_, Arc<Data>, AppError>,
    event: &FullEvent,
) -> Result<(), AppError> {
    match event {
        FullEvent::GuildMemberAddition { new_member } => {
            if framework
                .user_data()
                .await
                .db
                .is_user_banned(new_member.user.id)
                .await?
            {
                qurantine_user(
                    &framework.serenity_context,
                    framework.user_data.clone(),
                    new_member.guild_id,
                    new_member.user.id,
                    &new_member.roles,
                    true,
                )
                .await?;
                return Ok(());
            }
            if let Some(primary) = &new_member.user.primary_guild {
                if check_user_tag(framework.user_data.clone(), primary).await? {
                    qurantine_user(
                        &framework.serenity_context,
                        framework.user_data.clone(),
                        new_member.guild_id,
                        new_member.user.id,
                        &new_member.roles,
                        true,
                    )
                    .await?;
                }
            }
        }
        FullEvent::GuildMemberUpdate {
            old_if_available: _,
            new: _,
            event,
        } => {
            if framework.user_data.db.is_user_banned(event.user.id).await? {
                qurantine_user(
                    &framework.serenity_context,
                    framework.user_data.clone(),
                    event.guild_id,
                    event.user.id,
                    &event.roles,
                    false,
                )
                .await?;
                return Ok(());
            }

            if let Some(primary) = &event.user.primary_guild {
                if check_user_tag(framework.user_data.clone(), primary).await? {
                    warn!("matches!");
                    qurantine_user(
                        &framework.serenity_context,
                        framework.user_data.clone(),
                        event.guild_id,
                        event.user.id,
                        &event.roles,
                        true,
                    )
                    .await?;
                }
            }
        }
        FullEvent::Ready { data_about_bot: _ } => {
            log_wh(framework.user_data.clone(), "Ready".to_string()).await?;
        }
        FullEvent::UserUpdate { old_data, new } => {
            let guild_id = GuildId::new(framework.user_data.config.main_server);
            let roles = framework
                .serenity_context
                .http()
                .get_member(guild_id, new.id)
                .await?;

            if framework.user_data.db.is_user_banned(new.id).await? {
                qurantine_user(
                    &framework.serenity_context,
                    framework.user_data.clone(),
                    guild_id,
                    new.id,
                    &roles.roles,
                    false,
                )
                .await?;
                return Ok(());
            }

            if let Some(old) = old_data {
                if let Some(old_primary) = &old.primary_guild {
                    if check_user_tag(framework.user_data.clone(), old_primary).await? {
                        qurantine_user(
                            &framework.serenity_context,
                            framework.user_data.clone(),
                            guild_id,
                            old.id,
                            &roles.roles,
                            true,
                        )
                        .await?;
                        return Ok(());
                    }
                }
            }
            if let Some(primary) = &new.primary_guild {
                if check_user_tag(framework.user_data.clone(), primary).await? {
                    qurantine_user(
                        &framework.serenity_context,
                        framework.user_data.clone(),
                        guild_id,
                        new.id,
                        &roles.roles,
                        true,
                    )
                    .await?;
                }
            }
        }
        _ => return Ok(()),
    }
    Ok(())
}

async fn log_wh(data: Arc<Data>, text: String) -> Result<(), AppError> {
    info!("{}", &text);
    let mut builder = DiscordMessage::builder(&data.webhook);
    builder.add_message(text);
    let _ = builder.build().send().await;
    Ok(())
}

async fn check_user_tag(data: Arc<Data>, primary: &PrimaryGuild) -> Result<bool, AppError> {
    if let Some(tag) = &primary.tag {
        if data.db.is_tag_banned_by_tag(tag).await? {
            return Ok(true);
        }
    }
    if let Some(guild_id) = primary.identity_guild_id {
        if data.db.is_tag_banned_by_guild_id(guild_id).await? {
            return Ok(true);
        }
    }
    return Ok(false);
}

async fn qurantine_user(
    context: &SerenityContext,
    data: Arc<Data>,
    server_id: GuildId,
    user_id: UserId,
    roles: &[RoleId],
    add_to_ban: bool,
) -> Result<(), AppError> {
    let mut action_done = false;
    if data.db.is_user_exempted(user_id).await? {
        log_wh(
            data,
            format!(
                "Action: User Exempted on <@{}> ({})",
                user_id.get(),
                user_id.get()
            ),
        )
        .await?;
        return Ok(());
    }

    if let Some(add_role_id) = data.config.qurantine_add_role {
        let role_id = RoleId::new(add_role_id);
        if !roles.contains(&role_id) {
            context
                .http()
                .add_member_role(
                    server_id,
                    user_id,
                    role_id,
                    Some("Qurantine: Configured Add Role"),
                )
                .await?;
            action_done = true;
        }
    }
    if let Some(remove_role_id) = data.config.qurantine_prevent_role {
        let role_id = RoleId::new(remove_role_id);
        if roles.contains(&role_id) {
            context
                .http()
                .remove_member_role(
                    server_id,
                    user_id,
                    role_id,
                    Some("Qurantine: Configured Remove Role"),
                )
                .await?;
            action_done = true;
        }
    }

    if action_done {
        data.db.insert_action("qurantine", user_id).await?;
        log_wh(
            data.clone(),
            format!(
                "Action: Qurantine on <@{}> ({})",
                user_id.get(),
                user_id.get()
            ),
        )
        .await?;
    }

    if add_to_ban && !data.db.is_user_banned(user_id).await? {
        data.db.ban_user(user_id).await?;
    }

    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn status(context: Ctx<'_>) -> Result<(), AppError> {
    let (tags_count, users_count, bans_count, actions_count) =
        context.data().db.count_records().await?;
    let ping = context.ping().await.as_millis();

    let version = built_info::PKG_VERSION;

    context
        .reply(format!(
            r#"
```diff
Running nineteeneightyfour-yasl v{version}.
Created on @spunksuii's request for anti-goon squads.

++ Pong! Ping to discord took {ping}ms.

++ -- STATS --
++ Banned Tags:    {tags_count}
++ Banned Users:   {bans_count}
++ Exempted Users: {users_count}
++ Actions Taken:  {actions_count}
++ -----------

-- Julia's alibi. Winston's detested.
-- Licensed under GNU AGPL v3.0, (C) l1npengtul Twenty Twenty-Six.
-- Source Code: https://github.com/l1npengtul/yasl
```
            "#
        ))
        .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn exempt_user(context: Ctx<'_>, user: User) -> Result<(), AppError> {
    context.data().db.insert_exempt_user(user.id).await?;
    if context.data().db.is_user_banned(user.id).await? {
        context.data().db.unban_user(user.id).await?;
    }
    context
        .reply(format!(
            "Sucessfully exempted user {} ({}).",
            user.name, user.id
        ))
        .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn unexempt_user(context: Ctx<'_>, user: User) -> Result<(), AppError> {
    if !context.data().db.is_user_exempted(user.id).await? {
        context.reply("User is not exempt.").await?;
        return Ok(());
    }
    context.data().db.delete_exempted_user(user.id).await?;
    context
        .reply(format!(
            "Sucessfully unexempted user {} ({}).",
            user.name, user.id
        ))
        .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn exempted_users(context: Ctx<'_>) -> Result<(), AppError> {
    let mut table = Table::new("{:>} {<:}").with_heading("```");
    table.add_heading("Exempted Users:");
    table.add_row(row!("User ID", "Time (UTC)"));
    for (exempted_userid, time) in context.data().db.get_all_exempt_users().await? {
        table.add_row(row!(exempted_userid.get(), time.to_rfc3339()));
    }
    table.add_heading("```");
    context.reply(table.to_string()).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn add_banned_tag_by_user(context: Ctx<'_>, user: User) -> Result<(), AppError> {
    let (primary_id, primary_tag) = match user.primary_guild {
        Some(primary) => {
            let id = match primary.identity_guild_id {
                Some(id) => id,
                None => {
                    context
                        .reply(format!("Primary guild does not have an ID. Private guild?"))
                        .await?;
                    return Ok(());
                }
            };

            let tag = primary.tag;
            (id, tag)
        }
        None => {
            context
                .reply(format!(
                    "User {} does not have a primary guild.",
                    user.id.get()
                ))
                .await?;
            return Ok(());
        }
    };

    context
        .data()
        .db
        .insert_banned_tag(Some(primary_id), primary_tag)
        .await?;
    context.reply("Sucessfully applied new tag ban.").await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn add_banned_tag_by_guild_id(context: Ctx<'_>, guild_id: u64) -> Result<(), AppError> {
    context
        .data()
        .db
        .insert_banned_tag(Some(GuildId::new(guild_id)), None)
        .await?;
    context.reply("Sucessfully applied new tag ban.").await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn add_banned_tag_by_tag(context: Ctx<'_>, tag: String) -> Result<(), AppError> {
    let length = tag.chars().count();
    if length > 4 || length < 1 {
        context.reply("Invalid tag.").await?;
        return Ok(());
    }

    context.data().db.insert_banned_tag(None, Some(tag)).await?;
    context.reply("Sucessfully applied new tag ban.").await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn banned_tags(context: Ctx<'_>) -> Result<(), AppError> {
    let mut table = Table::new("{:>} {:<} {:<}").with_heading("Tag bans");
    table.add_heading("```");
    table.add_row(row!("ID", "ServerID", "Tag"));
    for (rowid, server_id, tag) in context.data().db.get_all_banned_tags().await? {
        table.add_row(row!(
            rowid,
            server_id.map(|x| x.to_string()).unwrap_or_default(),
            tag.unwrap_or_default()
        ));
    }
    table.add_heading("```");
    context.reply(table.to_string()).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn add_banned_user_manually(ctx: Ctx<'_>, user: UserId) -> Result<(), AppError> {
    ctx.data().db.ban_user(user).await?;
    ctx.reply("Banned user.").await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn banned_users(context: Ctx<'_>) -> Result<(), AppError> {
    let mut table = Table::new("{:>} {:<} {:<}").with_heading("User bans");
    table.add_heading("```");
    table.add_row(row!("ID", "UserID"));
    for (rowid, user_id) in context.data().db.get_all_banned_users().await? {
        table.add_row(row!(rowid, user_id.get()));
    }
    table.add_heading("```");
    context.reply(table.to_string()).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only, check = "check_permissions")]
async fn unban_tag(context: Ctx<'_>, id: i64) -> Result<(), AppError> {
    context.data().db.delete_banned_tag(id).await?;
    context.reply("Unbanned tag.").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
async fn register_commands(ctx: Ctx<'_>) -> Result<(), AppError> {
    let commands = &ctx.framework().options().commands;
    poise::builtins::register_globally(ctx.http(), commands).await?;

    ctx.say("Successfully registered slash commands!").await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
async fn stop(ctx: Ctx<'_>) -> Result<(), AppError> {
    log_wh(ctx.data().clone(), "Stopping...".to_string()).await?;
    ctx.serenity_context().shard.shutdown_clean();
    ctx.data().db.stop().await?;
    Ok(())
}

async fn handle_signals(mut signals: Signals, shard: Arc<ShardManager>, data: Arc<Data>) {
    while let Some(signal) = signals.next().await {
        match signal {
            SIGTERM | SIGINT | SIGQUIT => {
                shard.shutdown_all().await;
                data.db.stop().await.unwrap();
            }
            _ => unreachable!(),
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("starting yasl...");

    let yasl_test = std::env::var("YASLTEST").unwrap_or("0".to_string());

    let config = if yasl_test == "1" {
        info!("starting in test mode.");
        Figment::new()
            .merge(Toml::file("yasl.toml"))
            .extract::<Config>()
            .expect("Failed to load config file")
    } else {
        let configuration_directory =
            std::env::var("CONFIGURATION_DIRECTORY").unwrap_or("/etc".to_string());
        info!(
            "loading yasl config from directory: {}",
            &configuration_directory
        );

        Figment::new()
            .merge(Toml::file(format!("{configuration_directory}/yasl.toml")))
            .extract::<Config>()
            .expect("Failed to load config file")
    };

    let db_path = if yasl_test == "1" {
        "yasl.sql".to_string()
    } else {
        let state_directory =
            std::env::var("STATE_DIRECTORY").unwrap_or("/var/lib/yasl".to_string());
        info!("loading yasl database from directory: {}", &state_directory);
        format!("{state_directory}/yasl.sql")
    };

    let sqlite_connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);

    let db = Database {
        db: SqlitePool::connect_with(sqlite_connect_options)
            .await
            .unwrap(),
    };

    warn!("running db migrations");
    db.migrate().await.expect("failed to run db migrations!");

    warn!("reading discord token...");
    let discord_token = tokio::fs::read_to_string(&config.token_file)
        .await
        .expect("failed to read discord token file");

    warn!("reading discord webhook...");
    let webhook = tokio::fs::read_to_string(&config.log_channel_webhook_file)
        .await
        .expect("failed to read discord token file");
    info!("discord webhook url: {}", &webhook);

    let data = Arc::new(Data {
        config,
        db,
        webhook,
    });
    let data2 = data.clone();

    info!("hooking signals...");
    let signals = Signals::new(&[SIGTERM, SIGINT, SIGQUIT]).expect("failed to hook signals");
    let handle = signals.handle();

    let poise = Framework::builder()
        .options(FrameworkOptions {
            commands: vec![
                status(),
                exempt_user(),
                unexempt_user(),
                exempted_users(),
                banned_users(),
                add_banned_tag_by_user(),
                add_banned_tag_by_guild_id(),
                add_banned_tag_by_tag(),
                banned_tags(),
                unban_tag(),
                register_commands(),
                stop(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                non_command_message: Some(|_, _msg| Box::pin(async move { Ok(()) })),
                ..Default::default()
            },
            on_error: |err| Box::pin(async move { poise::builtins::on_error(err).await.unwrap() }),
            event_handler: |context, event| Box::pin(event_handler(context, event)),
            ..Default::default()
        })
        .setup(move |_ctx, ready, _framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);
                Ok(data)
            })
        })
        .build();

    let mut client = ClientBuilder::new(
        &discord_token,
        GatewayIntents::non_privileged()
            | GatewayIntents::GUILD_MEMBERS
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT,
    )
    .framework(poise)
    .status(OnlineStatus::Online)
    .await
    .expect("failed to log into discord");

    let shard_man = client.shard_manager.clone();
    tokio::spawn(handle_signals(signals, shard_man, data2));
    warn!("starting bot!");
    client.start().await.unwrap();
    handle.close();
}
