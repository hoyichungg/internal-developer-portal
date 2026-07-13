use std::fmt::{self, Display, Formatter};
use std::process::ExitCode;

use clap::{value_parser, Arg, ArgAction, ArgGroup, ArgMatches, Command};
use diesel::{sql_query, OptionalExtension, QueryableByName};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use serde_json::json;
use uuid::Uuid;

extern crate internal_developer_portal;

use internal_developer_portal::{
    config::{validate_test_database_url, verify_test_database_connection, AppConfig},
    validation::canonical_username,
};

#[derive(Debug)]
enum LinkEntraError {
    Config(String),
    DatabaseConnection,
    Database,
    UserNotFound(String),
    Conflict(String),
}

impl Display for LinkEntraError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) | Self::UserNotFound(message) | Self::Conflict(message) => {
                formatter.write_str(message)
            }
            Self::DatabaseConnection => formatter.write_str("cannot connect to Postgres"),
            Self::Database => formatter.write_str("database operation failed"),
        }
    }
}

impl From<diesel::result::Error> for LinkEntraError {
    fn from(_error: diesel::result::Error) -> Self {
        Self::Database
    }
}

#[derive(Clone)]
enum UserSelector {
    Id(i32),
    Username(String),
}

#[derive(QueryableByName)]
struct CliUser {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    username: String,
}

#[derive(QueryableByName)]
struct ExistingIdentity {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    user_id: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    issuer: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    subject: Option<String>,
}

#[derive(QueryableByName)]
struct InsertedIdentity {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
}

#[derive(Clone, Copy)]
enum LinkOutcome {
    Created,
    SubjectAdded,
    AlreadyLinked,
}

impl LinkOutcome {
    fn status(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::SubjectAdded => "subject-added",
            Self::AlreadyLinked => "already-linked",
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    dotenvy::dotenv().ok();

    let matches = build_cli().get_matches();
    let mut exit_code = ExitCode::SUCCESS;

    match matches.subcommand() {
        Some(("users", sub_matches)) => match sub_matches.subcommand() {
            Some(("create", sub_matches)) => {
                internal_developer_portal::commands::create_user(
                    sub_matches
                        .get_one::<String>("username")
                        .unwrap()
                        .to_owned(),
                    sub_matches
                        .get_one::<String>("password")
                        .unwrap()
                        .to_owned(),
                    sub_matches
                        .get_many::<String>("roles")
                        .unwrap()
                        .map(|v| v.to_owned())
                        .collect(),
                )
                .await
            }
            Some(("ensure-admin", sub_matches)) => {
                let username = sub_matches
                    .get_one::<String>("username")
                    .cloned()
                    .or_else(|| std::env::var("SEED_ADMIN_USERNAME").ok())
                    .unwrap_or_else(|| "admin".to_owned());
                let password = sub_matches
                    .get_one::<String>("password")
                    .cloned()
                    .or_else(|| std::env::var("SEED_ADMIN_PASSWORD").ok())
                    .unwrap_or_else(|| "admin123".to_owned());
                let roles = sub_matches
                    .get_many::<String>("roles")
                    .map(|roles| roles.map(ToOwned::to_owned).collect())
                    .or_else(|| {
                        std::env::var("SEED_ADMIN_ROLES")
                            .ok()
                            .map(|roles| vec![roles])
                    })
                    .unwrap_or_else(|| vec!["admin".to_owned(), "member".to_owned()]);
                let reset_password = sub_matches.get_flag("reset-password")
                    || std::env::var("SEED_ADMIN_RESET_PASSWORD").as_deref() == Ok("true");

                internal_developer_portal::commands::ensure_admin_user(
                    username,
                    password,
                    roles,
                    reset_password,
                )
                .await
            }
            Some(("list", _)) => internal_developer_portal::commands::list_users().await,
            Some(("delete", sub_matches)) => {
                internal_developer_portal::commands::delete_user(
                    sub_matches.get_one::<i32>("id").unwrap().to_owned(),
                )
                .await
            }
            Some(("link-entra", sub_matches)) => {
                if let Err(error) = run_link_entra(sub_matches).await {
                    eprintln!("Error: {error}");
                    exit_code = ExitCode::FAILURE;
                }
            }
            _ => {}
        },
        Some(("demo", sub_matches)) => {
            if let Some(("seed", sub_matches)) = sub_matches.subcommand() {
                internal_developer_portal::commands::seed_demo_data(
                    sub_matches.get_one::<String>("admin-username").cloned(),
                )
                .await
            }
        }
        _ => {}
    }

    exit_code
}

fn build_cli() -> Command {
    Command::new("internal-developer-portal")
        .about("Internal Developer Portal commands")
        .arg_required_else_help(true)
        .subcommand(
            Command::new("users")
                .about("User management")
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("create")
                        .about("Create a new user")
                        .arg_required_else_help(true)
                        .arg(Arg::new("username").required(true))
                        .arg(Arg::new("password").required(true))
                        .arg(
                            Arg::new("roles")
                                .required(true)
                                .num_args(1..)
                                .value_delimiter(','),
                        ),
                )
                .subcommand(
                    Command::new("ensure-admin")
                        .about("Ensure a seed admin user exists")
                        .arg(Arg::new("username").long("username"))
                        .arg(Arg::new("password").long("password"))
                        .arg(
                            Arg::new("roles")
                                .long("roles")
                                .num_args(1..)
                                .value_delimiter(','),
                        )
                        .arg(
                            Arg::new("reset-password")
                                .long("reset-password")
                                .action(ArgAction::SetTrue),
                        ),
                )
                .subcommand(Command::new("list").about("List existing users"))
                .subcommand(
                    Command::new("delete")
                        .about("Delete user by ID")
                        .arg_required_else_help(true)
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .value_parser(value_parser!(i32)),
                        ),
                )
                .subcommand(
                    Command::new("link-entra")
                        .about("Pre-link an existing user to an Entra object identity")
                        .arg_required_else_help(true)
                        .group(
                            ArgGroup::new("user-selector")
                                .required(true)
                                .multiple(false)
                                .args(["user-id", "username"]),
                        )
                        .arg(
                            Arg::new("user-id")
                                .long("user-id")
                                .value_name("ID")
                                .value_parser(value_parser!(i32).range(1..)),
                        )
                        .arg(Arg::new("username").long("username").value_name("USERNAME"))
                        .arg(
                            Arg::new("object-id")
                                .long("object-id")
                                .value_name("UUID")
                                .required(true)
                                .value_parser(value_parser!(Uuid)),
                        )
                        .arg(
                            Arg::new("subject")
                                .long("subject")
                                .value_name("SUBJECT")
                                .value_parser(parse_subject),
                        ),
                ),
        )
        .subcommand(
            Command::new("demo")
                .about("Demo data")
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("seed")
                        .about("Seed local demo portal data")
                        .arg(Arg::new("admin-username").long("admin-username")),
                ),
        )
}

fn parse_subject(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("subject must not be empty".to_owned());
    }
    if value.chars().count() > 255 {
        return Err("subject must be at most 255 characters".to_owned());
    }
    if value.chars().any(char::is_control) {
        return Err("subject must not contain control characters".to_owned());
    }

    Ok(value.to_owned())
}

async fn run_link_entra(matches: &ArgMatches) -> Result<(), LinkEntraError> {
    let selector = selected_user(matches)?;
    let object_id = matches
        .get_one::<Uuid>("object-id")
        .expect("clap validates object-id")
        .to_string();
    let subject = matches.get_one::<String>("subject").cloned();

    let config =
        AppConfig::from_env().map_err(|error| LinkEntraError::Config(error.to_string()))?;
    let environment = config.environment.clone();
    let entra = config.entra.ok_or_else(|| {
        LinkEntraError::Config(
            "Entra authentication must be enabled before identities can be linked".to_owned(),
        )
    })?;

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| LinkEntraError::Config("DATABASE_URL must be set".to_owned()))?;
    let test_target = validate_test_database_url(&environment, &database_url, "DATABASE_URL")
        .map_err(|error| LinkEntraError::Config(error.to_string()))?;
    let mut connection = AsyncPgConnection::establish(&database_url)
        .await
        .map_err(|_| LinkEntraError::DatabaseConnection)?;
    if let Some(target) = test_target.as_ref() {
        verify_test_database_connection(&mut connection, target, "DATABASE_URL")
            .await
            .map_err(|error| LinkEntraError::Config(error.to_string()))?;
    }

    let tenant_id = entra.tenant_id;
    let issuer = entra.issuer;
    let transaction_tenant_id = tenant_id.clone();
    let transaction_object_id = object_id.clone();

    let (user, outcome) = connection
        .transaction::<(CliUser, LinkOutcome), LinkEntraError, _>(|connection| {
            Box::pin(async move {
                let lock_key = format!("entra:{transaction_tenant_id}:{transaction_object_id}");
                sql_query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                    .bind::<diesel::sql_types::Text, _>(&lock_key)
                    .execute(connection)
                    .await?;

                let user = find_cli_user(connection, &selector).await?;
                let existing =
                    find_entra_identity(connection, &transaction_tenant_id, &transaction_object_id)
                        .await?;

                let outcome = match existing {
                    Some(identity) => {
                        resolve_existing_identity(
                            connection,
                            &identity,
                            &user,
                            &issuer,
                            subject.as_deref(),
                        )
                        .await?
                    }
                    None => {
                        create_entra_identity(
                            connection,
                            &user,
                            &issuer,
                            &transaction_tenant_id,
                            &transaction_object_id,
                            subject.as_deref(),
                        )
                        .await?
                    }
                };

                if !matches!(outcome, LinkOutcome::AlreadyLinked) {
                    write_link_audit(
                        connection,
                        &user,
                        &transaction_tenant_id,
                        &transaction_object_id,
                        outcome,
                    )
                    .await?;
                }

                Ok((user, outcome))
            })
        })
        .await?;

    println!(
        "Entra identity link status={} user_id={} username={} tenant_id={} object_id={}",
        outcome.status(),
        user.id,
        user.username,
        tenant_id,
        object_id
    );

    Ok(())
}

fn selected_user(matches: &ArgMatches) -> Result<UserSelector, LinkEntraError> {
    matches
        .get_one::<i32>("user-id")
        .copied()
        .map(UserSelector::Id)
        .or_else(|| {
            matches
                .get_one::<String>("username")
                .map(|username| UserSelector::Username(canonical_username(username)))
        })
        .ok_or_else(|| LinkEntraError::Config("a user selector is required".to_owned()))
}

async fn find_cli_user(
    connection: &mut AsyncPgConnection,
    selector: &UserSelector,
) -> Result<CliUser, LinkEntraError> {
    let result = match selector {
        UserSelector::Id(id) => {
            sql_query("SELECT id, username FROM users WHERE id = $1")
                .bind::<diesel::sql_types::Integer, _>(id)
                .get_result::<CliUser>(connection)
                .await
        }
        UserSelector::Username(username) => {
            let username = canonical_username(username);
            sql_query("SELECT id, username FROM users WHERE lower(username) = $1")
                .bind::<diesel::sql_types::Text, _>(username)
                .get_result::<CliUser>(connection)
                .await
        }
    };

    result.map_err(|error| match error {
        diesel::result::Error::NotFound => {
            LinkEntraError::UserNotFound("target user does not exist".to_owned())
        }
        _ => LinkEntraError::Database,
    })
}

async fn find_entra_identity(
    connection: &mut AsyncPgConnection,
    tenant_id: &str,
    object_id: &str,
) -> Result<Option<ExistingIdentity>, LinkEntraError> {
    sql_query(
        "SELECT id, user_id, issuer, subject FROM external_identities \
         WHERE provider = 'entra' AND tenant_id = $1 AND object_id = $2",
    )
    .bind::<diesel::sql_types::Text, _>(tenant_id)
    .bind::<diesel::sql_types::Text, _>(object_id)
    .get_result::<ExistingIdentity>(connection)
    .await
    .optional()
    .map_err(Into::into)
}

async fn find_entra_subject(
    connection: &mut AsyncPgConnection,
    issuer: &str,
    subject: &str,
) -> Result<Option<ExistingIdentity>, LinkEntraError> {
    sql_query(
        "SELECT id, user_id, issuer, subject FROM external_identities \
         WHERE provider = 'entra' AND issuer = $1 AND subject = $2",
    )
    .bind::<diesel::sql_types::Text, _>(issuer)
    .bind::<diesel::sql_types::Text, _>(subject)
    .get_result::<ExistingIdentity>(connection)
    .await
    .optional()
    .map_err(Into::into)
}

async fn resolve_existing_identity(
    connection: &mut AsyncPgConnection,
    identity: &ExistingIdentity,
    user: &CliUser,
    issuer: &str,
    requested_subject: Option<&str>,
) -> Result<LinkOutcome, LinkEntraError> {
    if identity.user_id != user.id {
        return Err(LinkEntraError::Conflict(format!(
            "Entra identity is already linked to a different user (user_id={})",
            identity.user_id
        )));
    }
    if identity.issuer != issuer {
        return Err(LinkEntraError::Conflict(
            "Entra identity issuer does not match the enabled configuration".to_owned(),
        ));
    }

    match (identity.subject.as_deref(), requested_subject) {
        (Some(existing), Some(requested)) if existing != requested => {
            Err(LinkEntraError::Conflict(
                "Entra identity is already linked with a different subject".to_owned(),
            ))
        }
        (None, Some(subject)) => {
            if find_entra_subject(connection, issuer, subject)
                .await?
                .is_some()
            {
                return Err(LinkEntraError::Conflict(
                    "Entra subject is already linked to another identity".to_owned(),
                ));
            }

            let updated = sql_query(
                "UPDATE external_identities SET subject = $1, updated_at = NOW() \
                 WHERE id = $2 AND (subject IS NULL OR subject = $1)",
            )
            .bind::<diesel::sql_types::Text, _>(subject)
            .bind::<diesel::sql_types::Integer, _>(identity.id)
            .execute(connection)
            .await?;
            if updated != 1 {
                return Err(LinkEntraError::Conflict(
                    "Entra identity subject changed concurrently; no link was modified".to_owned(),
                ));
            }
            Ok(LinkOutcome::SubjectAdded)
        }
        _ => Ok(LinkOutcome::AlreadyLinked),
    }
}

async fn create_entra_identity(
    connection: &mut AsyncPgConnection,
    user: &CliUser,
    issuer: &str,
    tenant_id: &str,
    object_id: &str,
    subject: Option<&str>,
) -> Result<LinkOutcome, LinkEntraError> {
    if let Some(subject) = subject {
        if find_entra_subject(connection, issuer, subject)
            .await?
            .is_some()
        {
            return Err(LinkEntraError::Conflict(
                "Entra subject is already linked to another identity".to_owned(),
            ));
        }
    }

    let inserted = sql_query(
        "INSERT INTO external_identities \
         (user_id, provider, issuer, subject, tenant_id, object_id) \
         VALUES ($1, 'entra', $2, $3, $4, $5) \
         ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind::<diesel::sql_types::Integer, _>(user.id)
    .bind::<diesel::sql_types::Text, _>(issuer)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(subject)
    .bind::<diesel::sql_types::Text, _>(tenant_id)
    .bind::<diesel::sql_types::Text, _>(object_id)
    .get_result::<InsertedIdentity>(connection)
    .await
    .optional()?;

    if let Some(inserted) = inserted {
        let _ = inserted.id;
        return Ok(LinkOutcome::Created);
    }

    let identity = find_entra_identity(connection, tenant_id, object_id)
        .await?
        .ok_or_else(|| {
            LinkEntraError::Conflict(
                "Entra issuer or subject is already linked to another identity".to_owned(),
            )
        })?;

    resolve_existing_identity(connection, &identity, user, issuer, subject).await
}

async fn write_link_audit(
    connection: &mut AsyncPgConnection,
    user: &CliUser,
    tenant_id: &str,
    object_id: &str,
    outcome: LinkOutcome,
) -> Result<(), LinkEntraError> {
    let action = match outcome {
        LinkOutcome::Created => "external_identity.linked",
        LinkOutcome::SubjectAdded => "external_identity.subject_added",
        LinkOutcome::AlreadyLinked => return Ok(()),
    };
    let resource_id = format!("entra:{tenant_id}:{object_id}");
    let metadata = json!({
        "operation": outcome.status(),
        "provider": "entra",
        "target_user_id": user.id,
        "target_username": user.username,
        "tenant_id": tenant_id,
        "object_id": object_id,
    })
    .to_string();

    sql_query(
        "INSERT INTO audit_logs \
         (actor_user_id, action, resource_type, resource_id, metadata) \
         VALUES (NULL, $1, 'external_identity', $2, $3)",
    )
    .bind::<diesel::sql_types::Text, _>(action)
    .bind::<diesel::sql_types::Text, _>(&resource_id)
    .bind::<diesel::sql_types::Text, _>(&metadata)
    .execute(connection)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;
    use uuid::Uuid;

    use super::{build_cli, selected_user, UserSelector};

    const OBJECT_ID: &str = "8f7459b2-4638-42c4-bbf4-c86f9028c58e";

    #[test]
    fn parses_entra_link_by_user_id_and_canonicalizes_uuid() {
        let matches = build_cli()
            .try_get_matches_from([
                "portal",
                "users",
                "link-entra",
                "--user-id",
                "42",
                "--object-id",
                "8F7459B2-4638-42C4-BBF4-C86F9028C58E",
                "--subject",
                "opaque-subject",
            ])
            .unwrap();
        let link = matches
            .subcommand_matches("users")
            .unwrap()
            .subcommand_matches("link-entra")
            .unwrap();

        assert_eq!(link.get_one::<i32>("user-id"), Some(&42));
        assert_eq!(
            link.get_one::<Uuid>("object-id").unwrap().to_string(),
            OBJECT_ID
        );
        assert_eq!(
            link.get_one::<String>("subject").map(String::as_str),
            Some("opaque-subject")
        );
    }

    #[test]
    fn parses_entra_link_by_username() {
        let matches = build_cli()
            .try_get_matches_from([
                "portal",
                "users",
                "link-entra",
                "--username",
                "alice",
                "--object-id",
                OBJECT_ID,
            ])
            .unwrap();
        let link = matches
            .subcommand_matches("users")
            .unwrap()
            .subcommand_matches("link-entra")
            .unwrap();

        assert_eq!(
            link.get_one::<String>("username").map(String::as_str),
            Some("alice")
        );

        match selected_user(link).expect("the CLI selector should be available") {
            UserSelector::Username(username) => assert_eq!(username, "alice"),
            UserSelector::Id(_) => panic!("expected a username selector"),
        }
    }

    #[test]
    fn canonicalizes_entra_link_username_selector() {
        let matches = build_cli()
            .try_get_matches_from([
                "portal",
                "users",
                "link-entra",
                "--username",
                "  Recovery.Admin  ",
                "--object-id",
                OBJECT_ID,
            ])
            .unwrap();
        let link = matches
            .subcommand_matches("users")
            .unwrap()
            .subcommand_matches("link-entra")
            .unwrap();

        match selected_user(link).expect("the CLI selector should be available") {
            UserSelector::Username(username) => assert_eq!(username, "recovery.admin"),
            UserSelector::Id(_) => panic!("expected a username selector"),
        }
    }

    #[test]
    fn rejects_missing_or_multiple_user_selectors() {
        let missing = build_cli()
            .try_get_matches_from(["portal", "users", "link-entra", "--object-id", OBJECT_ID])
            .unwrap_err();
        assert_eq!(missing.kind(), ErrorKind::MissingRequiredArgument);

        let multiple = build_cli()
            .try_get_matches_from([
                "portal",
                "users",
                "link-entra",
                "--user-id",
                "42",
                "--username",
                "alice",
                "--object-id",
                OBJECT_ID,
            ])
            .unwrap_err();
        assert_eq!(multiple.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn rejects_invalid_uuid_and_subject() {
        let invalid_uuid = build_cli()
            .try_get_matches_from([
                "portal",
                "users",
                "link-entra",
                "--user-id",
                "42",
                "--object-id",
                "not-a-uuid",
            ])
            .unwrap_err();
        assert_eq!(invalid_uuid.kind(), ErrorKind::ValueValidation);

        let invalid_subject = build_cli()
            .try_get_matches_from([
                "portal",
                "users",
                "link-entra",
                "--user-id",
                "42",
                "--object-id",
                OBJECT_ID,
                "--subject",
                "",
            ])
            .unwrap_err();
        assert_eq!(invalid_subject.kind(), ErrorKind::ValueValidation);
    }
}
