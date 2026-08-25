use rusqlite::{params, Row};

use crate::models::{
    NewRemoteMessage, RemoteIntent, RemoteMessage, RemoteProcessingStatus, RemoteReplayState,
};
use crate::store::{conv_err, dt, parse_enum, Store};
use crate::{time, Result};

fn from_row(row: &Row<'_>) -> rusqlite::Result<RemoteMessage> {
    let intent_raw: Option<String> = row.get("parsed_intent")?;
    Ok(RemoteMessage {
        id: row.get("id")?,
        channel: parse_enum(&row.get::<_, String>("channel")?)?,
        external_message_id: row.get("external_message_id")?,
        sender_id: row.get("sender_id")?,
        received_at: dt(row.get("received_at")?)?,
        raw_text: row.get("raw_text")?,
        authentication_state: parse_enum(&row.get::<_, String>("authentication_state")?)?,
        replay_state: parse_enum(&row.get::<_, String>("replay_state")?)?,
        parsed_intent: intent_raw
            .map(|s| serde_json::from_str::<RemoteIntent>(&s).map_err(conv_err))
            .transpose()?,
        resulting_inbox_item_id: row.get("resulting_inbox_item_id")?,
        processing_status: parse_enum(&row.get::<_, String>("processing_status")?)?,
        created_at: dt(row.get("created_at")?)?,
    })
}

impl Store {
    /// Remote mesajı kaydeder. `(channel, external_message_id)` tekildir:
    /// aynı mesajın ikinci teslimi `None` döner ve kayıt REPLAYED işaretlenir
    /// (replay koruması, docs/threat-model.md T5).
    pub fn record_remote_message(&self, input: NewRemoteMessage) -> Result<Option<RemoteMessage>> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::to_db(&time::now());
        let conn = self.db.conn();
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO remote_messages(id, channel, external_message_id, sender_id,
                received_at, raw_text, authentication_state, replay_state, parsed_intent,
                resulting_inbox_item_id, processing_status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,NULL,NULL,?9,?5)",
            params![
                id,
                input.channel.as_ref(),
                input.external_message_id,
                input.sender_id,
                now,
                input.raw_text,
                input.authentication_state.as_ref(),
                RemoteReplayState::New.as_ref(),
                RemoteProcessingStatus::Pending.as_ref(),
            ],
        )?;
        if inserted == 0 {
            conn.execute(
                "UPDATE remote_messages SET replay_state=?3
                 WHERE channel=?1 AND external_message_id=?2",
                params![
                    input.channel.as_ref(),
                    input.external_message_id,
                    RemoteReplayState::Replayed.as_ref()
                ],
            )?;
            return Ok(None);
        }
        drop(conn);
        Ok(Some(self.get_remote_message(&id)?))
    }

    pub fn get_remote_message(&self, id: &str) -> Result<RemoteMessage> {
        let conn = self.db.conn();
        Ok(conn.query_row("SELECT * FROM remote_messages WHERE id = ?1", [id], from_row)?)
    }

    pub fn finalize_remote_message(
        &self,
        id: &str,
        intent: Option<&RemoteIntent>,
        resulting_item: Option<&str>,
        status: RemoteProcessingStatus,
    ) -> Result<()> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE remote_messages SET parsed_intent=?2, resulting_inbox_item_id=?3,
                processing_status=?4 WHERE id=?1",
            params![
                id,
                intent.map(serde_json::to_string).transpose()?,
                resulting_item,
                status.as_ref(),
            ],
        )?;
        Ok(())
    }

    pub fn list_remote_messages(&self, limit: i64) -> Result<Vec<RemoteMessage>> {
        let conn = self.db.conn();
        let mut stmt =
            conn.prepare("SELECT * FROM remote_messages ORDER BY received_at DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit.clamp(1, 200)], from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
