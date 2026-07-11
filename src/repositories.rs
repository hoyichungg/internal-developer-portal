use chrono::{Duration, NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::OptionalExtension;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use std::collections::HashMap;

use crate::{models::*, schema::*};

#[derive(Clone, Debug)]
pub struct RecordAccessScope {
    pub user_id: i32,
    pub is_admin: bool,
    pub maintainer_ids: Vec<i32>,
}

impl RecordAccessScope {
    pub fn allows(&self, owner_user_id: Option<i32>, maintainer_id: Option<i32>) -> bool {
        if self.is_admin {
            return true;
        }

        match (owner_user_id, maintainer_id) {
            (Some(owner_user_id), None) => owner_user_id == self.user_id,
            (None, Some(maintainer_id)) => self.maintainer_ids.contains(&maintainer_id),
            (None, None) => true,
            (Some(_), Some(_)) => false,
        }
    }
}

pub struct ConnectorWorkerRepository;

impl ConnectorWorkerRepository {
    pub async fn upsert_heartbeat(
        c: &mut AsyncPgConnection,
        heartbeat: ConnectorWorkerHeartbeat,
    ) -> QueryResult<ConnectorWorker> {
        let now = Utc::now().naive_utc();
        let worker_id = heartbeat.worker_id.clone();

        diesel::insert_into(connector_workers::table)
            .values((
                connector_workers::worker_id.eq(worker_id),
                connector_workers::status.eq(heartbeat.status.clone()),
                connector_workers::scheduler_enabled.eq(heartbeat.scheduler_enabled),
                connector_workers::retention_enabled.eq(heartbeat.retention_enabled),
                connector_workers::current_run_id.eq(heartbeat.current_run_id),
                connector_workers::last_error.eq(heartbeat.last_error.clone()),
                connector_workers::started_at.eq(heartbeat.started_at),
                connector_workers::last_seen_at.eq(now),
                connector_workers::updated_at.eq(now),
            ))
            .on_conflict(connector_workers::worker_id)
            .do_update()
            .set((
                connector_workers::status.eq(heartbeat.status),
                connector_workers::scheduler_enabled.eq(heartbeat.scheduler_enabled),
                connector_workers::retention_enabled.eq(heartbeat.retention_enabled),
                connector_workers::current_run_id.eq(heartbeat.current_run_id),
                connector_workers::last_error.eq(heartbeat.last_error),
                connector_workers::last_seen_at.eq(now),
                connector_workers::updated_at.eq(now),
            ))
            .get_result(c)
            .await
    }

    pub async fn find_recent(
        c: &mut AsyncPgConnection,
        limit: i64,
    ) -> QueryResult<Vec<ConnectorWorker>> {
        connector_workers::table
            .order(connector_workers::last_seen_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }
}

pub struct ConnectorRepository;

impl ConnectorRepository {
    pub async fn find_multiple_for_access(
        c: &mut AsyncPgConnection,
        limit: i64,
        access: &RecordAccessScope,
    ) -> QueryResult<Vec<Connector>> {
        let mut query = connectors::table.into_boxed();
        if !access.is_admin {
            query = query.filter(
                connectors::owner_user_id
                    .eq(Some(access.user_id))
                    .or(connectors::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(connectors::owner_user_id
                        .is_null()
                        .and(connectors::maintainer_id.is_null())),
            );
        }

        query
            .order((
                connectors::updated_at.desc(),
                connectors::display_name.asc(),
            ))
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_by_source(c: &mut AsyncPgConnection, source: &str) -> QueryResult<Connector> {
        connectors::table
            .filter(connectors::source.eq(source))
            .first(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_connector: NewConnector,
    ) -> QueryResult<Connector> {
        diesel::insert_into(connectors::table)
            .values(new_connector)
            .get_result(c)
            .await
    }

    pub async fn find_or_create_default(
        c: &mut AsyncPgConnection,
        source: &str,
        kind: &str,
    ) -> QueryResult<Connector> {
        diesel::insert_into(connectors::table)
            .values(NewConnector {
                source: source.to_owned(),
                kind: kind.to_owned(),
                display_name: source.to_owned(),
                status: "active".to_owned(),
                scope_type: "global".to_owned(),
                owner_user_id: None,
                maintainer_id: None,
            })
            .on_conflict(connectors::source)
            .do_nothing()
            .execute(c)
            .await?;

        Self::find_by_source(c, source).await
    }

    pub async fn update_scope(
        c: &mut AsyncPgConnection,
        source: &str,
        scope_type: &str,
        owner_user_id: Option<i32>,
        maintainer_id: Option<i32>,
    ) -> QueryResult<Connector> {
        let source = source.to_owned();
        let scope_type = scope_type.to_owned();
        c.transaction::<Connector, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let connector = connectors::table
                    .filter(connectors::source.eq(&source))
                    .for_update()
                    .first::<Connector>(conn)
                    .await?;
                let connector = diesel::update(connectors::table.find(connector.id))
                    .set((
                        connectors::scope_type.eq(scope_type),
                        connectors::owner_user_id.eq(owner_user_id),
                        connectors::maintainer_id.eq(maintainer_id),
                        connectors::updated_at.eq(diesel::dsl::now),
                    ))
                    .get_result::<Connector>(conn)
                    .await?;

                diesel::update(work_cards::table.filter(work_cards::connector_id.eq(connector.id)))
                    .set((
                        work_cards::owner_user_id.eq(owner_user_id),
                        work_cards::maintainer_id.eq(maintainer_id),
                        work_cards::updated_at.eq(diesel::dsl::now),
                    ))
                    .execute(conn)
                    .await?;
                diesel::update(
                    notifications::table.filter(notifications::connector_id.eq(connector.id)),
                )
                .set((
                    notifications::owner_user_id.eq(owner_user_id),
                    notifications::maintainer_id.eq(maintainer_id),
                    notifications::updated_at.eq(diesel::dsl::now),
                ))
                .execute(conn)
                .await?;
                diesel::update(
                    calendar_events::table.filter(calendar_events::connector_id.eq(connector.id)),
                )
                .set((
                    calendar_events::owner_user_id.eq(owner_user_id),
                    calendar_events::maintainer_id.eq(maintainer_id),
                    calendar_events::updated_at.eq(diesel::dsl::now),
                ))
                .execute(conn)
                .await?;

                Ok(connector)
            })
        })
        .await
    }

    pub async fn update_by_source(
        c: &mut AsyncPgConnection,
        source: &str,
        connector: ConnectorUpdate,
    ) -> QueryResult<Connector> {
        diesel::update(connectors::table.filter(connectors::source.eq(source)))
            .set((connector, connectors::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn delete_by_source(c: &mut AsyncPgConnection, source: &str) -> QueryResult<usize> {
        diesel::delete(connectors::table.filter(connectors::source.eq(source)))
            .execute(c)
            .await
    }

    pub async fn touch_run_state(
        c: &mut AsyncPgConnection,
        source: &str,
        fallback_kind: &str,
        run_status: &str,
        finished_at: NaiveDateTime,
    ) -> QueryResult<Connector> {
        let connector_status = if run_status == "failed" {
            "error"
        } else {
            "active"
        };

        let success_at = if run_status == "success" {
            Some(finished_at)
        } else {
            None
        };

        match Self::find_by_source(c, source).await {
            Ok(_) if success_at.is_some() => {
                diesel::update(connectors::table.filter(connectors::source.eq(source)))
                    .set((
                        connectors::status.eq(connector_status),
                        connectors::last_run_at.eq(Some(finished_at)),
                        connectors::last_success_at.eq(success_at),
                        connectors::updated_at.eq(diesel::dsl::now),
                    ))
                    .get_result(c)
                    .await
            }
            Ok(_) => {
                diesel::update(connectors::table.filter(connectors::source.eq(source)))
                    .set((
                        connectors::status.eq(connector_status),
                        connectors::last_run_at.eq(Some(finished_at)),
                        connectors::updated_at.eq(diesel::dsl::now),
                    ))
                    .get_result(c)
                    .await
            }
            Err(diesel::result::Error::NotFound) => {
                diesel::insert_into(connectors::table)
                    .values((
                        connectors::source.eq(source),
                        connectors::kind.eq(fallback_kind),
                        connectors::display_name.eq(source),
                        connectors::status.eq(connector_status),
                        connectors::last_run_at.eq(Some(finished_at)),
                        connectors::last_success_at.eq(success_at),
                    ))
                    .get_result(c)
                    .await
            }
            Err(error) => Err(error),
        }
    }
}

pub struct ConnectorConfigRepository;

impl ConnectorConfigRepository {
    pub async fn find_by_source(
        c: &mut AsyncPgConnection,
        source: &str,
    ) -> QueryResult<ConnectorConfig> {
        connector_configs::table
            .filter(connector_configs::source.eq(source))
            .first(c)
            .await
    }

    pub async fn find_due_for_schedule(
        c: &mut AsyncPgConnection,
        now: NaiveDateTime,
        limit: i64,
    ) -> QueryResult<Vec<ConnectorConfig>> {
        connector_configs::table
            .filter(connector_configs::enabled.eq(true))
            .filter(connector_configs::schedule_cron.is_not_null())
            .filter(
                connector_configs::next_run_at
                    .is_null()
                    .or(connector_configs::next_run_at.le(now)),
            )
            .order(connector_configs::next_run_at.asc())
            .limit(limit)
            .for_update()
            .skip_locked()
            .get_results(c)
            .await
    }

    pub async fn upsert_by_source(
        c: &mut AsyncPgConnection,
        source: &str,
        config: ConnectorConfigUpdate,
    ) -> QueryResult<ConnectorConfig> {
        let next_run_at = if config.schedule_cron.is_some() {
            Some(Utc::now().naive_utc())
        } else {
            None
        };
        let target = config.target.clone();
        let enabled = config.enabled;
        let schedule_cron = config.schedule_cron.clone();
        let connector_config = config.config.clone();
        let sample_payload = config.sample_payload.clone();

        diesel::insert_into(connector_configs::table)
            .values(NewConnectorConfig {
                source: source.to_owned(),
                target,
                enabled,
                schedule_cron,
                config: connector_config,
                sample_payload,
                last_scheduled_at: None,
                next_run_at,
                last_scheduled_run_id: None,
            })
            .on_conflict(connector_configs::source)
            .do_update()
            .set((
                connector_configs::target.eq(config.target),
                connector_configs::enabled.eq(config.enabled),
                connector_configs::schedule_cron.eq(config.schedule_cron),
                connector_configs::config.eq(config.config),
                connector_configs::sample_payload.eq(config.sample_payload),
                connector_configs::next_run_at.eq(next_run_at),
                connector_configs::updated_at.eq(diesel::dsl::now),
            ))
            .get_result(c)
            .await
    }

    pub async fn update_config(
        c: &mut AsyncPgConnection,
        source: &str,
        config: String,
    ) -> QueryResult<ConnectorConfig> {
        diesel::update(connector_configs::table.filter(connector_configs::source.eq(source)))
            .set((
                connector_configs::config.eq(config),
                connector_configs::updated_at.eq(diesel::dsl::now),
            ))
            .get_result(c)
            .await
    }

    pub async fn mark_scheduled(
        c: &mut AsyncPgConnection,
        source: &str,
        scheduled_at: NaiveDateTime,
        interval_seconds: i64,
        run_id: Option<i32>,
    ) -> QueryResult<ConnectorConfig> {
        let next_run_at = scheduled_at + Duration::seconds(interval_seconds);

        diesel::update(connector_configs::table.filter(connector_configs::source.eq(source)))
            .set((
                connector_configs::last_scheduled_at.eq(Some(scheduled_at)),
                connector_configs::next_run_at.eq(Some(next_run_at)),
                connector_configs::last_scheduled_run_id.eq(run_id),
                connector_configs::updated_at.eq(diesel::dsl::now),
            ))
            .get_result(c)
            .await
    }
}

pub struct ConnectorRunRepository;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectorRunRecoveryStats {
    pub requeued: usize,
    pub failed: usize,
    pub cancelled: usize,
}

impl ConnectorRunRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<ConnectorRun> {
        connector_runs::table.find(id).get_result(c).await
    }

    pub async fn find_multiple(
        c: &mut AsyncPgConnection,
        limit: i64,
        source: Option<&str>,
        target: Option<&str>,
    ) -> QueryResult<Vec<ConnectorRun>> {
        let mut query = connector_runs::table.into_boxed();

        if let Some(source) = source {
            query = query.filter(connector_runs::source.eq(source));
        }

        if let Some(target) = target {
            query = query.filter(connector_runs::target.eq(target));
        }

        query
            .order(connector_runs::started_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_failed_for_access(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_id: Option<i32>,
        source: Option<&str>,
        access: &RecordAccessScope,
    ) -> QueryResult<Vec<ConnectorRun>> {
        let mut query = connector_runs::table
            .inner_join(connectors::table.on(connectors::source.eq(connector_runs::source)))
            .filter(connector_runs::status.eq_any(["failed", "partial_success"]))
            .select(connector_runs::all_columns)
            .into_boxed();
        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(connectors::maintainer_id.eq(maintainer_id));
        }
        if let Some(source) = source {
            query = query.filter(connector_runs::source.eq(source));
        }
        if !access.is_admin {
            query = query.filter(
                connectors::owner_user_id
                    .eq(Some(access.user_id))
                    .or(connectors::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(connectors::owner_user_id
                        .is_null()
                        .and(connectors::maintainer_id.is_null())),
            );
        }

        query
            .order(connector_runs::started_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_run: NewConnectorRun,
    ) -> QueryResult<ConnectorRun> {
        diesel::insert_into(connector_runs::table)
            .values(new_run)
            .get_result(c)
            .await
    }

    pub async fn create_if_no_pending(
        c: &mut AsyncPgConnection,
        new_run: NewConnectorRun,
    ) -> QueryResult<Option<ConnectorRun>> {
        let source = new_run.source.clone();
        let target = new_run.target.clone();
        c.transaction::<Option<ConnectorRun>, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                connectors::table
                    .filter(connectors::source.eq(&source))
                    .select(connectors::id)
                    .for_update()
                    .first::<i32>(conn)
                    .await?;
                let pending = connector_runs::table
                    .filter(connector_runs::source.eq(&source))
                    .filter(connector_runs::target.eq(&target))
                    .filter(connector_runs::status.eq_any(["queued", "running"]))
                    .count()
                    .get_result::<i64>(conn)
                    .await?
                    > 0;
                if pending {
                    return Ok(None);
                }

                diesel::insert_into(connector_runs::table)
                    .values(new_run)
                    .get_result(conn)
                    .await
                    .map(Some)
            })
        })
        .await
    }

    pub async fn create_retry(
        c: &mut AsyncPgConnection,
        parent_run_id: i32,
        new_run: NewConnectorRun,
    ) -> QueryResult<Option<ConnectorRun>> {
        let source = new_run.source.clone();
        let target = new_run.target.clone();
        c.transaction::<Option<ConnectorRun>, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                connectors::table
                    .filter(connectors::source.eq(&source))
                    .select(connectors::id)
                    .for_update()
                    .first::<i32>(conn)
                    .await?;
                connector_runs::table
                    .find(parent_run_id)
                    .select(connector_runs::id)
                    .for_update()
                    .first::<i32>(conn)
                    .await?;

                let active_run_exists = connector_runs::table
                    .filter(connector_runs::source.eq(&source))
                    .filter(connector_runs::target.eq(&target))
                    .filter(connector_runs::status.eq_any(["queued", "running"]))
                    .count()
                    .get_result::<i64>(conn)
                    .await?
                    > 0;
                if active_run_exists {
                    return Ok(None);
                }

                diesel::insert_into(connector_runs::table)
                    .values(new_run)
                    .get_result(conn)
                    .await
                    .map(Some)
            })
        })
        .await
    }

    pub async fn has_pending(
        c: &mut AsyncPgConnection,
        source: &str,
        target: &str,
    ) -> QueryResult<bool> {
        let count: i64 = connector_runs::table
            .filter(connector_runs::source.eq(source))
            .filter(connector_runs::target.eq(target))
            .filter(connector_runs::status.eq_any(["queued", "running"]))
            .count()
            .get_result(c)
            .await?;

        Ok(count > 0)
    }

    pub async fn claim_next_queued(
        c: &mut AsyncPgConnection,
        worker_id: &str,
        lease_seconds: i64,
    ) -> QueryResult<Option<ConnectorRun>> {
        c.transaction::<Option<ConnectorRun>, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let now = Utc::now().naive_utc();
                let queued = connector_runs::table
                    .filter(connector_runs::status.eq("queued"))
                    .filter(connector_runs::next_attempt_at.le(now))
                    .filter(connector_runs::cancel_requested_at.is_null())
                    .filter(connector_runs::attempt_count.lt(connector_runs::max_attempts))
                    .order((
                        connector_runs::next_attempt_at.asc(),
                        connector_runs::started_at.asc(),
                        connector_runs::id.asc(),
                    ))
                    .for_update()
                    .skip_locked()
                    .first::<ConnectorRun>(conn)
                    .await
                    .optional()?;

                let Some(queued) = queued else {
                    return Ok(None);
                };

                diesel::update(connector_runs::table.find(queued.id))
                    .set((
                        connector_runs::status.eq("running"),
                        connector_runs::claimed_at.eq(Some(now)),
                        connector_runs::worker_id.eq(Some(worker_id.to_owned())),
                        connector_runs::attempt_count.eq(queued.attempt_count + 1),
                        connector_runs::lease_expires_at
                            .eq(Some(now + Duration::seconds(lease_seconds))),
                        connector_runs::heartbeat_at.eq(Some(now)),
                        connector_runs::finished_at.eq::<Option<NaiveDateTime>>(None),
                        connector_runs::cancelled_at.eq::<Option<NaiveDateTime>>(None),
                    ))
                    .get_result(conn)
                    .await
                    .map(Some)
            })
        })
        .await
    }

    pub async fn renew_lease(
        c: &mut AsyncPgConnection,
        id: i32,
        worker_id: &str,
        lease_seconds: i64,
    ) -> QueryResult<bool> {
        let now = Utc::now().naive_utc();
        let updated = diesel::update(
            connector_runs::table
                .find(id)
                .filter(connector_runs::status.eq("running"))
                .filter(connector_runs::worker_id.eq(worker_id))
                .filter(connector_runs::cancel_requested_at.is_null())
                .filter(connector_runs::lease_expires_at.gt(now)),
        )
        .set((
            connector_runs::heartbeat_at.eq(Some(now)),
            connector_runs::lease_expires_at.eq(Some(now + Duration::seconds(lease_seconds))),
        ))
        .execute(c)
        .await?;

        Ok(updated == 1)
    }

    pub async fn execution_is_active(
        c: &mut AsyncPgConnection,
        id: i32,
        worker_id: Option<&str>,
    ) -> QueryResult<bool> {
        let mut query = connector_runs::table
            .find(id)
            .filter(connector_runs::status.eq("running"))
            .filter(connector_runs::cancel_requested_at.is_null())
            .into_boxed();
        if let Some(worker_id) = worker_id {
            let now = Utc::now().naive_utc();
            query = query
                .filter(connector_runs::worker_id.eq(worker_id))
                .filter(connector_runs::lease_expires_at.gt(now));
        } else {
            query = query.filter(connector_runs::worker_id.is_null());
        }

        Ok(query.count().get_result::<i64>(c).await? == 1)
    }

    pub async fn cancellation_requested(c: &mut AsyncPgConnection, id: i32) -> QueryResult<bool> {
        Ok(connector_runs::table
            .find(id)
            .filter(connector_runs::cancel_requested_at.is_not_null())
            .count()
            .get_result::<i64>(c)
            .await?
            == 1)
    }

    pub async fn finalize_if_active(
        c: &mut AsyncPgConnection,
        id: i32,
        worker_id: Option<&str>,
        state: ConnectorRunStateUpdate,
    ) -> QueryResult<Option<ConnectorRun>> {
        let now = Utc::now().naive_utc();
        let changes = (
            connector_runs::status.eq(state.status),
            connector_runs::success_count.eq(state.success_count),
            connector_runs::failure_count.eq(state.failure_count),
            connector_runs::duration_ms.eq(state.duration_ms),
            connector_runs::error_message.eq(state.error_message),
            connector_runs::finished_at.eq(state.finished_at),
            connector_runs::snapshot_complete.eq(state.snapshot_complete),
            connector_runs::archived_count.eq(state.archived_count),
            connector_runs::lease_expires_at.eq::<Option<NaiveDateTime>>(None),
            connector_runs::heartbeat_at.eq(Some(now)),
        );

        if let Some(worker_id) = worker_id {
            diesel::update(
                connector_runs::table
                    .find(id)
                    .filter(connector_runs::status.eq("running"))
                    .filter(connector_runs::worker_id.eq(worker_id))
                    .filter(connector_runs::cancel_requested_at.is_null())
                    .filter(connector_runs::lease_expires_at.gt(now)),
            )
            .set(changes)
            .get_result(c)
            .await
            .optional()
        } else {
            diesel::update(
                connector_runs::table
                    .find(id)
                    .filter(connector_runs::status.eq("running"))
                    .filter(connector_runs::worker_id.is_null())
                    .filter(connector_runs::cancel_requested_at.is_null()),
            )
            .set(changes)
            .get_result(c)
            .await
            .optional()
        }
    }

    pub async fn request_cancel(
        c: &mut AsyncPgConnection,
        id: i32,
    ) -> QueryResult<Option<ConnectorRun>> {
        c.transaction::<Option<ConnectorRun>, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let run = connector_runs::table
                    .find(id)
                    .for_update()
                    .first::<ConnectorRun>(conn)
                    .await?;
                let now = Utc::now().naive_utc();

                match run.status.as_str() {
                    "queued" => diesel::update(connector_runs::table.find(id))
                        .set((
                            connector_runs::status.eq("cancelled"),
                            connector_runs::cancel_requested_at.eq(Some(now)),
                            connector_runs::cancelled_at.eq(Some(now)),
                            connector_runs::finished_at.eq(Some(now)),
                            connector_runs::lease_expires_at.eq::<Option<NaiveDateTime>>(None),
                        ))
                        .get_result(conn)
                        .await
                        .map(Some),
                    "running" => diesel::update(connector_runs::table.find(id))
                        .set(connector_runs::cancel_requested_at.eq(Some(now)))
                        .get_result(conn)
                        .await
                        .map(Some),
                    _ => Ok(None),
                }
            })
        })
        .await
    }

    pub async fn mark_cancelled_if_requested(
        c: &mut AsyncPgConnection,
        id: i32,
        worker_id: Option<&str>,
    ) -> QueryResult<Option<ConnectorRun>> {
        let now = Utc::now().naive_utc();
        let changes = (
            connector_runs::status.eq("cancelled"),
            connector_runs::cancelled_at.eq(Some(now)),
            connector_runs::finished_at.eq(Some(now)),
            connector_runs::lease_expires_at.eq::<Option<NaiveDateTime>>(None),
            connector_runs::heartbeat_at.eq(Some(now)),
            connector_runs::error_message.eq(Some("cancelled by operator".to_owned())),
        );
        if let Some(worker_id) = worker_id {
            diesel::update(
                connector_runs::table
                    .find(id)
                    .filter(connector_runs::status.eq("running"))
                    .filter(connector_runs::worker_id.eq(worker_id))
                    .filter(connector_runs::cancel_requested_at.is_not_null()),
            )
            .set(changes)
            .get_result(c)
            .await
            .optional()
        } else {
            diesel::update(
                connector_runs::table
                    .find(id)
                    .filter(connector_runs::status.eq("running"))
                    .filter(connector_runs::worker_id.is_null())
                    .filter(connector_runs::cancel_requested_at.is_not_null()),
            )
            .set(changes)
            .get_result(c)
            .await
            .optional()
        }
    }

    pub async fn recover_expired_leases(
        c: &mut AsyncPgConnection,
        retry_base_seconds: i64,
        retry_max_seconds: i64,
        limit: i64,
    ) -> QueryResult<ConnectorRunRecoveryStats> {
        c.transaction::<ConnectorRunRecoveryStats, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let now = Utc::now().naive_utc();
                let expired = connector_runs::table
                    .filter(connector_runs::status.eq("running"))
                    .filter(connector_runs::lease_expires_at.le(now))
                    .order((connector_runs::lease_expires_at.asc(), connector_runs::id.asc()))
                    .limit(limit)
                    .for_update()
                    .skip_locked()
                    .get_results::<ConnectorRun>(conn)
                    .await?;
                let mut stats = ConnectorRunRecoveryStats::default();

                for run in expired {
                    if run.cancel_requested_at.is_some() {
                        diesel::update(connector_runs::table.find(run.id))
                            .set((
                                connector_runs::status.eq("cancelled"),
                                connector_runs::cancelled_at.eq(Some(now)),
                                connector_runs::finished_at.eq(Some(now)),
                                connector_runs::lease_expires_at
                                    .eq::<Option<NaiveDateTime>>(None),
                                connector_runs::error_message
                                    .eq(Some("cancelled after worker lease expired".to_owned())),
                            ))
                            .execute(conn)
                            .await?;
                        stats.cancelled += 1;
                    } else if run.attempt_count >= run.max_attempts {
                        diesel::update(connector_runs::table.find(run.id))
                            .set((
                                connector_runs::status.eq("failed"),
                                connector_runs::finished_at.eq(Some(now)),
                                connector_runs::lease_expires_at
                                    .eq::<Option<NaiveDateTime>>(None),
                                connector_runs::error_message.eq(Some(format!(
                                    "worker lease expired after {} attempt(s); retry limit reached",
                                    run.attempt_count
                                ))),
                            ))
                            .execute(conn)
                            .await?;
                        diesel::update(
                            connectors::table.filter(connectors::source.eq(&run.source)),
                        )
                        .set((
                            connectors::status.eq("error"),
                            connectors::last_run_at.eq(Some(now)),
                            connectors::updated_at.eq(now),
                        ))
                        .execute(conn)
                        .await?;
                        stats.failed += 1;
                    } else {
                        let backoff_seconds = bounded_retry_backoff_seconds(
                            run.attempt_count,
                            retry_base_seconds,
                            retry_max_seconds,
                        );
                        diesel::update(connector_runs::table.find(run.id))
                            .set((
                                connector_runs::status.eq("queued"),
                                connector_runs::next_attempt_at
                                    .eq(now + Duration::seconds(backoff_seconds)),
                                connector_runs::claimed_at.eq::<Option<NaiveDateTime>>(None),
                                connector_runs::worker_id.eq::<Option<String>>(None),
                                connector_runs::lease_expires_at
                                    .eq::<Option<NaiveDateTime>>(None),
                                connector_runs::heartbeat_at.eq::<Option<NaiveDateTime>>(None),
                                connector_runs::error_message.eq(Some(format!(
                                    "worker lease expired on attempt {}; retry scheduled in {} second(s)",
                                    run.attempt_count, backoff_seconds
                                ))),
                            ))
                            .execute(conn)
                            .await?;
                        stats.requeued += 1;
                    }
                }

                Ok(stats)
            })
        })
        .await
    }

    pub async fn update_state(
        c: &mut AsyncPgConnection,
        id: i32,
        state: ConnectorRunStateUpdate,
    ) -> QueryResult<ConnectorRun> {
        diesel::update(connector_runs::table.find(id))
            .set((
                connector_runs::status.eq(state.status),
                connector_runs::success_count.eq(state.success_count),
                connector_runs::failure_count.eq(state.failure_count),
                connector_runs::duration_ms.eq(state.duration_ms),
                connector_runs::error_message.eq(state.error_message),
                connector_runs::finished_at.eq(state.finished_at),
                connector_runs::snapshot_complete.eq(state.snapshot_complete),
                connector_runs::archived_count.eq(state.archived_count),
                connector_runs::lease_expires_at.eq::<Option<NaiveDateTime>>(None),
                connector_runs::heartbeat_at.eq(Some(Utc::now().naive_utc())),
            ))
            .get_result(c)
            .await
    }

    pub async fn delete_finished_older_than(
        c: &mut AsyncPgConnection,
        finished_before: NaiveDateTime,
    ) -> QueryResult<usize> {
        diesel::delete(
            connector_runs::table
                .filter(connector_runs::finished_at.is_not_null())
                .filter(connector_runs::finished_at.lt(finished_before)),
        )
        .execute(c)
        .await
    }
}

pub(crate) fn bounded_retry_backoff_seconds(
    attempt_count: i32,
    base_seconds: i64,
    max_seconds: i64,
) -> i64 {
    let base_seconds = base_seconds.max(1);
    let max_seconds = max_seconds.max(base_seconds);
    let exponent = attempt_count.saturating_sub(1).clamp(0, 30) as u32;
    base_seconds
        .saturating_mul(1_i64 << exponent)
        .min(max_seconds)
}

pub struct ConnectorRunItemErrorRepository;

impl ConnectorRunItemErrorRepository {
    pub async fn find_by_run(
        c: &mut AsyncPgConnection,
        connector_run_id: i32,
    ) -> QueryResult<Vec<ConnectorRunItemError>> {
        connector_run_item_errors::table
            .filter(connector_run_item_errors::connector_run_id.eq(connector_run_id))
            .order(connector_run_item_errors::id.asc())
            .get_results(c)
            .await
    }

    pub async fn create_many(
        c: &mut AsyncPgConnection,
        new_errors: Vec<NewConnectorRunItemError>,
    ) -> QueryResult<Vec<ConnectorRunItemError>> {
        if new_errors.is_empty() {
            return Ok(Vec::new());
        }

        diesel::insert_into(connector_run_item_errors::table)
            .values(new_errors)
            .get_results(c)
            .await
    }
}

pub struct ConnectorRunItemRepository;

impl ConnectorRunItemRepository {
    pub async fn find_by_run(
        c: &mut AsyncPgConnection,
        connector_run_id: i32,
    ) -> QueryResult<Vec<ConnectorRunItem>> {
        connector_run_items::table
            .filter(connector_run_items::connector_run_id.eq(connector_run_id))
            .order(connector_run_items::id.asc())
            .get_results(c)
            .await
    }

    pub async fn create_many(
        c: &mut AsyncPgConnection,
        new_items: Vec<NewConnectorRunItem>,
    ) -> QueryResult<Vec<ConnectorRunItem>> {
        if new_items.is_empty() {
            return Ok(Vec::new());
        }

        diesel::insert_into(connector_run_items::table)
            .values(new_items)
            .get_results(c)
            .await
    }
}

pub struct MaintenanceRunRepository;

impl MaintenanceRunRepository {
    pub async fn create(
        c: &mut AsyncPgConnection,
        new_maintenance_run: NewMaintenanceRun,
    ) -> QueryResult<MaintenanceRun> {
        diesel::insert_into(maintenance_runs::table)
            .values(new_maintenance_run)
            .get_result(c)
            .await
    }

    pub async fn find_recent(
        c: &mut AsyncPgConnection,
        limit: i64,
        task: Option<&str>,
    ) -> QueryResult<Vec<MaintenanceRun>> {
        let mut query = maintenance_runs::table.into_boxed();

        if let Some(task) = task {
            query = query.filter(maintenance_runs::task.eq(task));
        }

        query
            .order(maintenance_runs::created_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_latest_success(
        c: &mut AsyncPgConnection,
        task: &str,
    ) -> QueryResult<Option<MaintenanceRun>> {
        maintenance_runs::table
            .filter(maintenance_runs::task.eq(task))
            .filter(maintenance_runs::status.eq("success"))
            .order(maintenance_runs::created_at.desc())
            .first(c)
            .await
            .optional()
    }
}

pub struct AuditLogRepository;

pub struct AuditLogFilters<'a> {
    pub resource_type: Option<&'a str>,
    pub resource_id: Option<&'a str>,
    pub actor_user_id: Option<i32>,
    pub action: Option<&'a str>,
    pub created_from: Option<NaiveDateTime>,
    pub created_to: Option<NaiveDateTime>,
}

impl AuditLogRepository {
    pub async fn find_multiple(
        c: &mut AsyncPgConnection,
        limit: i64,
        filters: AuditLogFilters<'_>,
    ) -> QueryResult<Vec<AuditLog>> {
        let mut query = audit_logs::table.into_boxed();

        if let Some(resource_type) = filters.resource_type {
            query = query.filter(audit_logs::resource_type.eq(resource_type));
        }

        if let Some(resource_id) = filters.resource_id {
            query = query.filter(audit_logs::resource_id.eq(resource_id));
        }

        if let Some(actor_user_id) = filters.actor_user_id {
            query = query.filter(audit_logs::actor_user_id.eq(actor_user_id));
        }

        if let Some(action) = filters.action {
            query = query.filter(audit_logs::action.eq(action));
        }

        if let Some(created_from) = filters.created_from {
            query = query.filter(audit_logs::created_at.ge(created_from));
        }

        if let Some(created_to) = filters.created_to {
            query = query.filter(audit_logs::created_at.le(created_to));
        }

        query
            .order(audit_logs::created_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_audit_log: NewAuditLog,
    ) -> QueryResult<AuditLog> {
        diesel::insert_into(audit_logs::table)
            .values(new_audit_log)
            .get_result(c)
            .await
    }

    pub async fn delete_older_than(
        c: &mut AsyncPgConnection,
        created_before: NaiveDateTime,
    ) -> QueryResult<usize> {
        diesel::delete(audit_logs::table.filter(audit_logs::created_at.lt(created_before)))
            .execute(c)
            .await
    }
}

pub struct MaintainerRepository;

impl MaintainerRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<Maintainer> {
        maintainers::table.find(id).get_result(c).await
    }

    pub async fn find_multiple(
        c: &mut AsyncPgConnection,
        limit: i64,
    ) -> QueryResult<Vec<Maintainer>> {
        maintainers::table
            .order(maintainers::display_name.asc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_by_ids(
        c: &mut AsyncPgConnection,
        ids: &[i32],
    ) -> QueryResult<Vec<Maintainer>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        maintainers::table
            .filter(maintainers::id.eq_any(ids))
            .order(maintainers::display_name.asc())
            .get_results(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_maintainer: NewMaintainer,
    ) -> QueryResult<Maintainer> {
        diesel::insert_into(maintainers::table)
            .values(new_maintainer)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        maintainer: NewMaintainer,
    ) -> QueryResult<Maintainer> {
        diesel::update(maintainers::table.find(id))
            .set(maintainer)
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(maintainer_members::table.filter(maintainer_members::maintainer_id.eq(id)))
            .execute(c)
            .await?;

        diesel::delete(maintainers::table.find(id)).execute(c).await
    }
}

pub struct MaintainerMemberRepository;

impl MaintainerMemberRepository {
    pub async fn find_by_user(
        c: &mut AsyncPgConnection,
        user_id: i32,
    ) -> QueryResult<Vec<MaintainerMember>> {
        maintainer_members::table
            .filter(maintainer_members::user_id.eq(user_id))
            .order(maintainer_members::maintainer_id.asc())
            .get_results(c)
            .await
    }

    pub async fn find_by_maintainer(
        c: &mut AsyncPgConnection,
        maintainer_id: i32,
    ) -> QueryResult<Vec<MaintainerMember>> {
        maintainer_members::table
            .filter(maintainer_members::maintainer_id.eq(maintainer_id))
            .order(maintainer_members::user_id.asc())
            .get_results(c)
            .await
    }

    pub async fn find_by_maintainer_and_user(
        c: &mut AsyncPgConnection,
        maintainer_id: i32,
        user_id: i32,
    ) -> QueryResult<MaintainerMember> {
        maintainer_members::table
            .filter(maintainer_members::maintainer_id.eq(maintainer_id))
            .filter(maintainer_members::user_id.eq(user_id))
            .first(c)
            .await
    }

    pub async fn upsert(
        c: &mut AsyncPgConnection,
        new_member: NewMaintainerMember,
    ) -> QueryResult<MaintainerMember> {
        let role = new_member.role.clone();

        diesel::insert_into(maintainer_members::table)
            .values(new_member)
            .on_conflict((
                maintainer_members::maintainer_id,
                maintainer_members::user_id,
            ))
            .do_update()
            .set(maintainer_members::role.eq(role))
            .get_result(c)
            .await
    }

    pub async fn delete_by_maintainer_and_user(
        c: &mut AsyncPgConnection,
        maintainer_id: i32,
        user_id: i32,
    ) -> QueryResult<usize> {
        diesel::delete(
            maintainer_members::table
                .filter(maintainer_members::maintainer_id.eq(maintainer_id))
                .filter(maintainer_members::user_id.eq(user_id)),
        )
        .execute(c)
        .await
    }
}

pub struct PackageRepository;

impl PackageRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<Package> {
        packages::table.find(id).get_result(c).await
    }

    pub async fn find_multiple(c: &mut AsyncPgConnection, limit: i64) -> QueryResult<Vec<Package>> {
        packages::table
            .order(packages::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_recent_for_maintainer(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_id: Option<i32>,
    ) -> QueryResult<Vec<Package>> {
        let mut query = packages::table.into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(packages::maintainer_id.eq(maintainer_id));
        }

        query
            .order(packages::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_recent_for_maintainers(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_ids: &[i32],
    ) -> QueryResult<Vec<Package>> {
        if maintainer_ids.is_empty() {
            return Ok(Vec::new());
        }

        packages::table
            .filter(packages::maintainer_id.eq_any(maintainer_ids))
            .order(packages::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_package: NewPackage,
    ) -> QueryResult<Package> {
        diesel::insert_into(packages::table)
            .values(new_package)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        package: NewPackage,
    ) -> QueryResult<Package> {
        diesel::update(packages::table.find(id))
            .set((package, packages::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(packages::table.find(id)).execute(c).await
    }
}

pub struct ServiceRepository;

impl ServiceRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<Service> {
        services::table.find(id).get_result(c).await
    }

    pub async fn find_multiple(c: &mut AsyncPgConnection, limit: i64) -> QueryResult<Vec<Service>> {
        services::table
            .order(services::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_for_maintainers(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_ids: &[i32],
    ) -> QueryResult<Vec<Service>> {
        if maintainer_ids.is_empty() {
            return Ok(Vec::new());
        }

        services::table
            .filter(services::maintainer_id.eq_any(maintainer_ids))
            .order(services::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_health_snapshot_scoped(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_id: Option<i32>,
        source: Option<&str>,
    ) -> QueryResult<Vec<Service>> {
        let mut query = services::table.into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(services::maintainer_id.eq(maintainer_id));
        }

        if let Some(source) = source {
            query = query.filter(services::source.eq(source));
        }

        query
            .order(services::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_service: NewService,
    ) -> QueryResult<Service> {
        diesel::insert_into(services::table)
            .values(new_service)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        service: NewService,
    ) -> QueryResult<Service> {
        diesel::update(services::table.find(id))
            .set((service, services::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(services::table.find(id)).execute(c).await
    }

    pub async fn upsert_from_connector_with_health_check(
        c: &mut AsyncPgConnection,
        service: NewService,
        connector_run_id: i32,
        raw_payload: Option<String>,
    ) -> QueryResult<Service> {
        let source = service.source.clone();
        let external_id = service.external_id.clone();

        c.transaction::<Service, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let existing = match external_id.as_deref() {
                    Some(external_id) => services::table
                        .filter(services::source.eq(&source))
                        .filter(services::external_id.eq(external_id))
                        .first::<Service>(conn)
                        .await
                        .optional()?,
                    None => None,
                };
                let previous_health_status = existing
                    .as_ref()
                    .map(|service| service.health_status.clone());
                let service = match existing {
                    Some(existing) => Self::update(conn, existing.id, service).await?,
                    None => Self::create(conn, service).await?,
                };
                let checked_at = service
                    .last_checked_at
                    .unwrap_or_else(|| Utc::now().naive_utc());

                ServiceHealthCheckRepository::create(
                    conn,
                    NewServiceHealthCheck {
                        service_id: service.id,
                        connector_run_id: Some(connector_run_id),
                        source: service.source.clone(),
                        external_id: service.external_id.clone(),
                        health_status: service.health_status.clone(),
                        previous_health_status,
                        checked_at,
                        response_time_ms: None,
                        message: None,
                        raw_payload,
                    },
                )
                .await?;

                Ok(service)
            })
        })
        .await
    }
}

pub struct ServiceHealthCheckRepository;

impl ServiceHealthCheckRepository {
    pub async fn create(
        c: &mut AsyncPgConnection,
        new_check: NewServiceHealthCheck,
    ) -> QueryResult<ServiceHealthCheck> {
        diesel::insert_into(service_health_checks::table)
            .values(new_check)
            .get_result(c)
            .await
    }

    pub async fn find_recent_scoped(
        c: &mut AsyncPgConnection,
        limit: i64,
        since: NaiveDateTime,
        maintainer_id: Option<i32>,
        source: Option<&str>,
    ) -> QueryResult<Vec<ServiceHealthCheck>> {
        let mut query = service_health_checks::table
            .inner_join(services::table)
            .select(service_health_checks::all_columns)
            .filter(service_health_checks::checked_at.ge(since))
            .into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(services::maintainer_id.eq(maintainer_id));
        }

        if let Some(source) = source {
            query = query.filter(service_health_checks::source.eq(source));
        }

        query
            .order(service_health_checks::checked_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_by_run(
        c: &mut AsyncPgConnection,
        connector_run_id: i32,
    ) -> QueryResult<Vec<ServiceHealthCheck>> {
        service_health_checks::table
            .filter(service_health_checks::connector_run_id.eq(connector_run_id))
            .order(service_health_checks::checked_at.desc())
            .get_results(c)
            .await
    }

    pub async fn find_recent_for_maintainers(
        c: &mut AsyncPgConnection,
        limit: i64,
        since: NaiveDateTime,
        maintainer_ids: &[i32],
    ) -> QueryResult<Vec<ServiceHealthCheck>> {
        if maintainer_ids.is_empty() {
            return Ok(Vec::new());
        }

        service_health_checks::table
            .inner_join(services::table)
            .select(service_health_checks::all_columns)
            .filter(service_health_checks::checked_at.ge(since))
            .filter(services::maintainer_id.eq_any(maintainer_ids))
            .order(service_health_checks::checked_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn delete_older_than(
        c: &mut AsyncPgConnection,
        checked_before: NaiveDateTime,
    ) -> QueryResult<usize> {
        diesel::delete(
            service_health_checks::table
                .filter(service_health_checks::checked_at.lt(checked_before)),
        )
        .execute(c)
        .await
    }
}

pub struct WorkCardRepository;

pub struct CalendarEventRepository;

impl CalendarEventRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<CalendarEvent> {
        calendar_events::table.find(id).get_result(c).await
    }

    pub async fn find_upcoming_for_access(
        c: &mut AsyncPgConnection,
        limit: i64,
        starts_from: NaiveDateTime,
        starts_before: NaiveDateTime,
        maintainer_id: Option<i32>,
        source: Option<&str>,
        access: &RecordAccessScope,
    ) -> QueryResult<Vec<CalendarEvent>> {
        let mut query = calendar_events::table
            .filter(calendar_events::archived_at.is_null())
            .filter(calendar_events::is_cancelled.eq(false))
            .filter(calendar_events::starts_at.ge(starts_from))
            .filter(calendar_events::starts_at.lt(starts_before))
            .into_boxed();
        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(calendar_events::maintainer_id.eq(maintainer_id));
        }
        if let Some(source) = source {
            query = query.filter(calendar_events::source.eq(source));
        }
        if !access.is_admin {
            query = query.filter(
                calendar_events::owner_user_id
                    .eq(Some(access.user_id))
                    .or(calendar_events::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(calendar_events::owner_user_id
                        .is_null()
                        .and(calendar_events::maintainer_id.is_null())),
            );
        }

        query
            .order((calendar_events::starts_at.asc(), calendar_events::id.asc()))
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_by_source_external_id(
        c: &mut AsyncPgConnection,
        source: &str,
        external_id: &str,
    ) -> QueryResult<CalendarEvent> {
        calendar_events::table
            .filter(calendar_events::source.eq(source))
            .filter(calendar_events::external_id.eq(external_id))
            .first(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        event: NewCalendarEvent,
    ) -> QueryResult<CalendarEvent> {
        diesel::insert_into(calendar_events::table)
            .values(event)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        event: NewCalendarEvent,
    ) -> QueryResult<CalendarEvent> {
        diesel::update(calendar_events::table.find(id))
            .set((event, calendar_events::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn upsert_from_connector(
        c: &mut AsyncPgConnection,
        event: NewCalendarEvent,
    ) -> QueryResult<CalendarEvent> {
        match Self::find_by_source_external_id(c, &event.source, &event.external_id).await {
            Ok(existing) => Self::update(c, existing.id, event).await,
            Err(diesel::result::Error::NotFound) => Self::create(c, event).await,
            Err(error) => Err(error),
        }
    }

    pub async fn archive_missing_for_connector_run(
        c: &mut AsyncPgConnection,
        connector_id: i32,
        run_id: i32,
    ) -> QueryResult<usize> {
        let now = Utc::now().naive_utc();
        diesel::update(
            calendar_events::table
                .filter(calendar_events::connector_id.eq(connector_id))
                .filter(calendar_events::archived_at.is_null())
                .filter(
                    calendar_events::last_seen_run_id
                        .is_null()
                        .or(calendar_events::last_seen_run_id.ne(run_id)),
                ),
        )
        .set((
            calendar_events::archived_at.eq(Some(now)),
            calendar_events::updated_at.eq(now),
        ))
        .execute(c)
        .await
    }
}

impl WorkCardRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<WorkCard> {
        work_cards::table.find(id).get_result(c).await
    }

    pub async fn find_multiple_for_access(
        c: &mut AsyncPgConnection,
        limit: i64,
        access: &RecordAccessScope,
    ) -> QueryResult<Vec<WorkCard>> {
        let mut query = work_cards::table
            .filter(work_cards::archived_at.is_null())
            .into_boxed();
        if !access.is_admin {
            query = query.filter(
                work_cards::owner_user_id
                    .eq(Some(access.user_id))
                    .or(work_cards::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(work_cards::owner_user_id
                        .is_null()
                        .and(work_cards::maintainer_id.is_null())),
            );
        }

        query
            .order(work_cards::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn find_open_for_access(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_id: Option<i32>,
        source: Option<&str>,
        access: &RecordAccessScope,
    ) -> QueryResult<Vec<WorkCard>> {
        let mut query = work_cards::table
            .filter(work_cards::status.ne("done"))
            .filter(work_cards::archived_at.is_null())
            .into_boxed();
        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(work_cards::maintainer_id.eq(maintainer_id));
        }
        if let Some(source) = source {
            query = query.filter(work_cards::source.eq(source));
        }
        if !access.is_admin {
            query = query.filter(
                work_cards::owner_user_id
                    .eq(Some(access.user_id))
                    .or(work_cards::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(work_cards::owner_user_id
                        .is_null()
                        .and(work_cards::maintainer_id.is_null())),
            );
        }

        query
            .order(work_cards::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await
    }

    pub async fn count_open_for_access(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
        source: Option<&str>,
        access: &RecordAccessScope,
    ) -> QueryResult<i64> {
        let mut query = work_cards::table
            .filter(work_cards::status.ne("done"))
            .filter(work_cards::archived_at.is_null())
            .into_boxed();
        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(work_cards::maintainer_id.eq(maintainer_id));
        }
        if let Some(source) = source {
            query = query.filter(work_cards::source.eq(source));
        }
        if !access.is_admin {
            query = query.filter(
                work_cards::owner_user_id
                    .eq(Some(access.user_id))
                    .or(work_cards::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(work_cards::owner_user_id
                        .is_null()
                        .and(work_cards::maintainer_id.is_null())),
            );
        }

        query.count().get_result(c).await
    }

    pub async fn find_by_source_external_id(
        c: &mut AsyncPgConnection,
        source: &str,
        external_id: &str,
    ) -> QueryResult<WorkCard> {
        work_cards::table
            .filter(work_cards::source.eq(source))
            .filter(work_cards::external_id.eq(external_id))
            .first(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_work_card: NewWorkCard,
    ) -> QueryResult<WorkCard> {
        diesel::insert_into(work_cards::table)
            .values(new_work_card)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        work_card: NewWorkCard,
    ) -> QueryResult<WorkCard> {
        diesel::update(work_cards::table.find(id))
            .set((work_card, work_cards::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(work_cards::table.find(id)).execute(c).await
    }

    pub async fn upsert_from_connector(
        c: &mut AsyncPgConnection,
        work_card: NewWorkCard,
    ) -> QueryResult<WorkCard> {
        if let Some(external_id) = work_card.external_id.clone() {
            match Self::find_by_source_external_id(c, &work_card.source, &external_id).await {
                Ok(existing) => Self::update(c, existing.id, work_card).await,
                Err(diesel::result::Error::NotFound) => Self::create(c, work_card).await,
                Err(error) => Err(error),
            }
        } else {
            Self::create(c, work_card).await
        }
    }

    pub async fn archive_missing_for_connector_run(
        c: &mut AsyncPgConnection,
        connector_id: i32,
        run_id: i32,
    ) -> QueryResult<usize> {
        let now = Utc::now().naive_utc();
        diesel::update(
            work_cards::table
                .filter(work_cards::connector_id.eq(connector_id))
                .filter(work_cards::archived_at.is_null())
                .filter(
                    work_cards::last_seen_run_id
                        .is_null()
                        .or(work_cards::last_seen_run_id.ne(run_id)),
                ),
        )
        .set((
            work_cards::archived_at.eq(Some(now)),
            work_cards::updated_at.eq(now),
        ))
        .execute(c)
        .await
    }
}

pub struct NotificationReceiptRepository;

impl NotificationReceiptRepository {
    pub async fn find(
        c: &mut AsyncPgConnection,
        notification_id: i32,
        user_id: i32,
    ) -> QueryResult<Option<NotificationReceipt>> {
        notification_receipts::table
            .filter(notification_receipts::notification_id.eq(notification_id))
            .filter(notification_receipts::user_id.eq(user_id))
            .first(c)
            .await
            .optional()
    }

    pub async fn find_for_user_and_notifications(
        c: &mut AsyncPgConnection,
        user_id: i32,
        notification_ids: &[i32],
    ) -> QueryResult<Vec<NotificationReceipt>> {
        if notification_ids.is_empty() {
            return Ok(Vec::new());
        }

        notification_receipts::table
            .filter(notification_receipts::user_id.eq(user_id))
            .filter(notification_receipts::notification_id.eq_any(notification_ids))
            .get_results(c)
            .await
    }

    pub async fn mark_read(
        c: &mut AsyncPgConnection,
        notification_id: i32,
        user_id: i32,
    ) -> QueryResult<NotificationReceipt> {
        let now = Utc::now().naive_utc();

        diesel::insert_into(notification_receipts::table)
            .values((
                notification_receipts::notification_id.eq(notification_id),
                notification_receipts::user_id.eq(user_id),
                notification_receipts::read_at.eq(Some(now)),
                notification_receipts::dismissed_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::snoozed_until.eq::<Option<NaiveDateTime>>(None),
            ))
            .on_conflict((
                notification_receipts::notification_id,
                notification_receipts::user_id,
            ))
            .do_update()
            .set((
                notification_receipts::read_at.eq(Some(now)),
                notification_receipts::updated_at.eq(now),
            ))
            .get_result(c)
            .await
    }

    pub async fn mark_unread(
        c: &mut AsyncPgConnection,
        notification_id: i32,
        user_id: i32,
    ) -> QueryResult<NotificationReceipt> {
        let now = Utc::now().naive_utc();

        diesel::insert_into(notification_receipts::table)
            .values((
                notification_receipts::notification_id.eq(notification_id),
                notification_receipts::user_id.eq(user_id),
                notification_receipts::read_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::dismissed_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::snoozed_until.eq::<Option<NaiveDateTime>>(None),
            ))
            .on_conflict((
                notification_receipts::notification_id,
                notification_receipts::user_id,
            ))
            .do_update()
            .set((
                notification_receipts::read_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::updated_at.eq(now),
            ))
            .get_result(c)
            .await
    }

    pub async fn dismiss(
        c: &mut AsyncPgConnection,
        notification_id: i32,
        user_id: i32,
    ) -> QueryResult<NotificationReceipt> {
        let now = Utc::now().naive_utc();

        diesel::insert_into(notification_receipts::table)
            .values((
                notification_receipts::notification_id.eq(notification_id),
                notification_receipts::user_id.eq(user_id),
                notification_receipts::read_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::dismissed_at.eq(Some(now)),
                notification_receipts::snoozed_until.eq::<Option<NaiveDateTime>>(None),
            ))
            .on_conflict((
                notification_receipts::notification_id,
                notification_receipts::user_id,
            ))
            .do_update()
            .set((
                notification_receipts::dismissed_at.eq(Some(now)),
                notification_receipts::snoozed_until.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::updated_at.eq(now),
            ))
            .get_result(c)
            .await
    }

    pub async fn snooze(
        c: &mut AsyncPgConnection,
        notification_id: i32,
        user_id: i32,
        snoozed_until: NaiveDateTime,
    ) -> QueryResult<NotificationReceipt> {
        let now = Utc::now().naive_utc();

        diesel::insert_into(notification_receipts::table)
            .values((
                notification_receipts::notification_id.eq(notification_id),
                notification_receipts::user_id.eq(user_id),
                notification_receipts::read_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::dismissed_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::snoozed_until.eq(Some(snoozed_until)),
            ))
            .on_conflict((
                notification_receipts::notification_id,
                notification_receipts::user_id,
            ))
            .do_update()
            .set((
                notification_receipts::dismissed_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::snoozed_until.eq(Some(snoozed_until)),
                notification_receipts::updated_at.eq(now),
            ))
            .get_result(c)
            .await
    }

    pub async fn restore(
        c: &mut AsyncPgConnection,
        notification_id: i32,
        user_id: i32,
    ) -> QueryResult<NotificationReceipt> {
        let now = Utc::now().naive_utc();

        diesel::insert_into(notification_receipts::table)
            .values((
                notification_receipts::notification_id.eq(notification_id),
                notification_receipts::user_id.eq(user_id),
                notification_receipts::read_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::dismissed_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::snoozed_until.eq::<Option<NaiveDateTime>>(None),
            ))
            .on_conflict((
                notification_receipts::notification_id,
                notification_receipts::user_id,
            ))
            .do_update()
            .set((
                notification_receipts::dismissed_at.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::snoozed_until.eq::<Option<NaiveDateTime>>(None),
                notification_receipts::updated_at.eq(now),
            ))
            .get_result(c)
            .await
    }
}

pub struct NotificationRepository;

impl NotificationRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<Notification> {
        notifications::table.find(id).get_result(c).await
    }

    pub async fn find_actionable_for_access(
        c: &mut AsyncPgConnection,
        limit: i64,
        maintainer_id: Option<i32>,
        source: Option<&str>,
        access: &RecordAccessScope,
    ) -> QueryResult<Vec<NotificationView>> {
        let now = Utc::now().naive_utc();
        let mut query = notifications::table
            .left_join(
                notification_receipts::table.on(notification_receipts::notification_id
                    .eq(notifications::id)
                    .and(notification_receipts::user_id.eq(access.user_id))),
            )
            .filter(notifications::is_read.eq(false))
            .filter(notifications::archived_at.is_null())
            .filter(notification_receipts::read_at.is_null())
            .filter(notification_receipts::dismissed_at.is_null())
            .filter(
                notification_receipts::snoozed_until
                    .is_null()
                    .or(notification_receipts::snoozed_until.le(now)),
            )
            .select(notifications::all_columns)
            .into_boxed();
        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(notifications::maintainer_id.eq(maintainer_id));
        }
        if let Some(source) = source {
            query = query.filter(notifications::source.eq(source));
        }
        if !access.is_admin {
            query = query.filter(
                notifications::owner_user_id
                    .eq(Some(access.user_id))
                    .or(notifications::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(notifications::owner_user_id
                        .is_null()
                        .and(notifications::maintainer_id.is_null())),
            );
        }

        let notifications = query
            .order(notifications::updated_at.desc())
            .limit(limit)
            .get_results(c)
            .await?;

        Self::with_receipts(c, access.user_id, notifications).await
    }

    pub async fn count_actionable_for_access(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
        source: Option<&str>,
        access: &RecordAccessScope,
    ) -> QueryResult<i64> {
        let now = Utc::now().naive_utc();
        let mut query = notifications::table
            .left_join(
                notification_receipts::table.on(notification_receipts::notification_id
                    .eq(notifications::id)
                    .and(notification_receipts::user_id.eq(access.user_id))),
            )
            .filter(notifications::is_read.eq(false))
            .filter(notifications::archived_at.is_null())
            .filter(notification_receipts::read_at.is_null())
            .filter(notification_receipts::dismissed_at.is_null())
            .filter(
                notification_receipts::snoozed_until
                    .is_null()
                    .or(notification_receipts::snoozed_until.le(now)),
            )
            .into_boxed();
        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(notifications::maintainer_id.eq(maintainer_id));
        }
        if let Some(source) = source {
            query = query.filter(notifications::source.eq(source));
        }
        if !access.is_admin {
            query = query.filter(
                notifications::owner_user_id
                    .eq(Some(access.user_id))
                    .or(notifications::maintainer_id.eq_any(&access.maintainer_ids))
                    .or(notifications::owner_user_id
                        .is_null()
                        .and(notifications::maintainer_id.is_null())),
            );
        }

        query.count().get_result(c).await
    }

    pub async fn view_for_user(
        c: &mut AsyncPgConnection,
        notification: Notification,
        user_id: i32,
    ) -> QueryResult<NotificationView> {
        let receipt = NotificationReceiptRepository::find(c, notification.id, user_id).await?;
        Ok(NotificationView::from_record(notification, receipt))
    }

    async fn with_receipts(
        c: &mut AsyncPgConnection,
        user_id: i32,
        notifications: Vec<Notification>,
    ) -> QueryResult<Vec<NotificationView>> {
        let notification_ids = notifications
            .iter()
            .map(|notification| notification.id)
            .collect::<Vec<_>>();
        let receipts = NotificationReceiptRepository::find_for_user_and_notifications(
            c,
            user_id,
            &notification_ids,
        )
        .await?;
        let mut receipts_by_notification = receipts
            .into_iter()
            .map(|receipt| (receipt.notification_id, receipt))
            .collect::<HashMap<_, _>>();

        Ok(notifications
            .into_iter()
            .map(|notification| {
                let receipt = receipts_by_notification.remove(&notification.id);
                NotificationView::from_record(notification, receipt)
            })
            .collect())
    }

    pub async fn find_by_source_external_id(
        c: &mut AsyncPgConnection,
        source: &str,
        external_id: &str,
    ) -> QueryResult<Notification> {
        notifications::table
            .filter(notifications::source.eq(source))
            .filter(notifications::external_id.eq(external_id))
            .first(c)
            .await
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_notification: NewNotification,
    ) -> QueryResult<Notification> {
        diesel::insert_into(notifications::table)
            .values(new_notification)
            .get_result(c)
            .await
    }

    pub async fn update(
        c: &mut AsyncPgConnection,
        id: i32,
        notification: NewNotification,
    ) -> QueryResult<Notification> {
        diesel::update(notifications::table.find(id))
            .set((notification, notifications::updated_at.eq(diesel::dsl::now)))
            .get_result(c)
            .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(notifications::table.find(id))
            .execute(c)
            .await
    }

    pub async fn upsert_from_connector(
        c: &mut AsyncPgConnection,
        notification: NewNotification,
    ) -> QueryResult<Notification> {
        if let Some(external_id) = notification.external_id.clone() {
            match Self::find_by_source_external_id(c, &notification.source, &external_id).await {
                Ok(existing) => Self::update(c, existing.id, notification).await,
                Err(diesel::result::Error::NotFound) => Self::create(c, notification).await,
                Err(error) => Err(error),
            }
        } else {
            Self::create(c, notification).await
        }
    }

    pub async fn archive_missing_for_connector_run(
        c: &mut AsyncPgConnection,
        connector_id: i32,
        run_id: i32,
    ) -> QueryResult<usize> {
        let now = Utc::now().naive_utc();
        diesel::update(
            notifications::table
                .filter(notifications::connector_id.eq(connector_id))
                .filter(notifications::archived_at.is_null())
                .filter(
                    notifications::last_seen_run_id
                        .is_null()
                        .or(notifications::last_seen_run_id.ne(run_id)),
                ),
        )
        .set((
            notifications::archived_at.eq(Some(now)),
            notifications::updated_at.eq(now),
        ))
        .execute(c)
        .await
    }
}

pub struct DashboardRepository;

impl DashboardRepository {
    pub async fn total_services_scoped(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
        source: Option<&str>,
    ) -> QueryResult<i64> {
        let mut query = services::table.into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(services::maintainer_id.eq(maintainer_id));
        }

        if let Some(source) = source {
            query = query.filter(services::source.eq(source));
        }

        query.count().get_result(c).await
    }

    pub async fn healthy_services_scoped(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
        source: Option<&str>,
    ) -> QueryResult<i64> {
        let mut query = services::table
            .filter(services::health_status.eq("healthy"))
            .into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(services::maintainer_id.eq(maintainer_id));
        }

        if let Some(source) = source {
            query = query.filter(services::source.eq(source));
        }

        query.count().get_result(c).await
    }

    pub async fn degraded_services_scoped(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
        source: Option<&str>,
    ) -> QueryResult<i64> {
        let mut query = services::table
            .filter(services::health_status.eq("degraded"))
            .into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(services::maintainer_id.eq(maintainer_id));
        }

        if let Some(source) = source {
            query = query.filter(services::source.eq(source));
        }

        query.count().get_result(c).await
    }

    pub async fn down_services_scoped(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
        source: Option<&str>,
    ) -> QueryResult<i64> {
        let mut query = services::table
            .filter(services::health_status.eq("down"))
            .into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(services::maintainer_id.eq(maintainer_id));
        }

        if let Some(source) = source {
            query = query.filter(services::source.eq(source));
        }

        query.count().get_result(c).await
    }

    pub async fn active_packages_scoped(
        c: &mut AsyncPgConnection,
        maintainer_id: Option<i32>,
    ) -> QueryResult<i64> {
        let mut query = packages::table
            .filter(packages::status.eq("active"))
            .into_boxed();

        if let Some(maintainer_id) = maintainer_id {
            query = query.filter(packages::maintainer_id.eq(maintainer_id));
        }

        query.count().get_result(c).await
    }
}

pub struct UserRepository;

impl UserRepository {
    pub async fn find(c: &mut AsyncPgConnection, id: i32) -> QueryResult<User> {
        users::table.find(id).get_result(c).await
    }

    pub async fn find_by_username(c: &mut AsyncPgConnection, username: &str) -> QueryResult<User> {
        users::table
            .filter(users::username.eq(username))
            .get_result(c)
            .await
    }
    pub async fn find_with_roles(
        c: &mut AsyncPgConnection,
    ) -> QueryResult<Vec<(User, Vec<(UserRole, Role)>)>> {
        let users = users::table.load::<User>(c).await?;
        let result = users_roles::table
            .inner_join(roles::table)
            .load::<(UserRole, Role)>(c)
            .await?
            .grouped_by(&users);

        Ok(users.into_iter().zip(result).collect())
    }

    pub async fn create(
        c: &mut AsyncPgConnection,
        new_user: NewUser,
        role_codes: Vec<String>,
    ) -> QueryResult<User> {
        c.transaction::<_, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                let user = diesel::insert_into(users::table)
                    .values(new_user)
                    .get_result::<User>(conn)
                    .await?;

                for role_code in role_codes {
                    let role = match RoleRepository::find_by_code(conn, &role_code).await {
                        Ok(role) => role,
                        Err(diesel::result::Error::NotFound) => {
                            let new_role = NewRole {
                                code: role_code.to_owned(),
                                name: role_code.to_owned(),
                            };
                            match RoleRepository::create(conn, new_role).await {
                                Ok(role) => role,
                                Err(diesel::result::Error::DatabaseError(
                                    diesel::result::DatabaseErrorKind::UniqueViolation,
                                    _,
                                )) => RoleRepository::find_by_code(conn, &role_code).await?,
                                Err(error) => return Err(error),
                            }
                        }
                        Err(e) => return Err(e),
                    };

                    let new_user_role = NewUserRole {
                        user_id: user.id,
                        role_id: role.id,
                    };

                    diesel::insert_into(users_roles::table)
                        .values(new_user_role)
                        .get_result::<UserRole>(conn)
                        .await?;
                }

                Ok(user)
            })
        })
        .await
    }

    pub async fn delete(c: &mut AsyncPgConnection, id: i32) -> QueryResult<usize> {
        diesel::delete(maintainer_members::table.filter(maintainer_members::user_id.eq(id)))
            .execute(c)
            .await?;

        diesel::delete(users_roles::table.filter(users_roles::user_id.eq(id)))
            .execute(c)
            .await?;

        diesel::delete(users::table.find(id)).execute(c).await
    }

    pub async fn update_password(
        c: &mut AsyncPgConnection,
        id: i32,
        password_hash: String,
    ) -> QueryResult<User> {
        diesel::update(users::table.find(id))
            .set(users::password.eq(password_hash))
            .get_result(c)
            .await
    }
}

pub struct SessionRepository;

impl SessionRepository {
    pub async fn create(
        c: &mut AsyncPgConnection,
        user_id: i32,
        token: String,
        expires_at: NaiveDateTime,
    ) -> QueryResult<Session> {
        diesel::insert_into(sessions::table)
            .values(NewSession {
                user_id,
                token,
                expires_at,
            })
            .get_result(c)
            .await
    }

    pub async fn find_by_token(c: &mut AsyncPgConnection, token: &str) -> QueryResult<Session> {
        sessions::table
            .filter(sessions::token.eq(token))
            .first::<Session>(c)
            .await
    }

    pub async fn delete_by_token(c: &mut AsyncPgConnection, token: &str) -> QueryResult<usize> {
        diesel::delete(sessions::table.filter(sessions::token.eq(token)))
            .execute(c)
            .await
    }
}

pub struct RoleRepository;

impl RoleRepository {
    pub async fn find_by_ids(c: &mut AsyncPgConnection, ids: Vec<i32>) -> QueryResult<Vec<Role>> {
        roles::table.filter(roles::id.eq_any(ids)).load(c).await
    }

    pub async fn find_by_code(c: &mut AsyncPgConnection, code: &str) -> QueryResult<Role> {
        roles::table.filter(roles::code.eq(code)).first(c).await
    }

    pub async fn find_by_user(c: &mut AsyncPgConnection, user: &User) -> QueryResult<Vec<Role>> {
        let user_roles = UserRole::belonging_to(&user)
            .get_results::<UserRole>(c)
            .await?;
        let role_ids: Vec<i32> = user_roles.iter().map(|ur: &UserRole| ur.role_id).collect();

        Self::find_by_ids(c, role_ids).await
    }

    pub async fn create(c: &mut AsyncPgConnection, new_role: NewRole) -> QueryResult<Role> {
        diesel::insert_into(roles::table)
            .values(new_role)
            .get_result(c)
            .await
    }

    pub async fn find_or_create_by_code(
        c: &mut AsyncPgConnection,
        code: &str,
    ) -> QueryResult<Role> {
        match Self::find_by_code(c, code).await {
            Ok(role) => Ok(role),
            Err(diesel::result::Error::NotFound) => {
                let new_role = NewRole {
                    code: code.to_owned(),
                    name: code.to_owned(),
                };

                match Self::create(c, new_role).await {
                    Ok(role) => Ok(role),
                    Err(diesel::result::Error::DatabaseError(
                        diesel::result::DatabaseErrorKind::UniqueViolation,
                        _,
                    )) => Self::find_by_code(c, code).await,
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }
}

pub struct UserRoleRepository;

impl UserRoleRepository {
    pub async fn assign_if_missing(
        c: &mut AsyncPgConnection,
        user_id: i32,
        role_id: i32,
    ) -> QueryResult<Option<UserRole>> {
        let existing = users_roles::table
            .filter(users_roles::user_id.eq(user_id))
            .filter(users_roles::role_id.eq(role_id))
            .first::<UserRole>(c)
            .await
            .optional()?;

        if existing.is_some() {
            return Ok(None);
        }

        diesel::insert_into(users_roles::table)
            .values(NewUserRole { user_id, role_id })
            .get_result(c)
            .await
            .map(Some)
    }
}
