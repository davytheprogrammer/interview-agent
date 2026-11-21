use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session { pub id: String, pub kind: String, pub transcript: String, pub notes: String, pub updated_at: i64 }
pub struct Store(Mutex<Connection>);
impl Store {
 pub fn open(path: &Path) -> Result<Self,String> { Self::init(Connection::open(path).map_err(|_| "Could not open local database")?) }
 fn init(db: Connection) -> Result<Self,String> {
  db.execute_batch("PRAGMA secure_delete=ON; PRAGMA foreign_keys=ON;").map_err(|_| "Could not initialize local database")?;
  let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|_| "Could not read database version")?;
  if version > 1 { return Err("Database was created by a newer version of Live Companion".into()); }
  if version == 0 { db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); CREATE TABLE sessions (id TEXT PRIMARY KEY, kind TEXT NOT NULL, transcript TEXT NOT NULL, notes TEXT NOT NULL, updated_at INTEGER NOT NULL); PRAGMA user_version=1; COMMIT;").map_err(|_| "Could not migrate local database")?; }
  Ok(Self(Mutex::new(db)))
 }
 fn db(&self) -> Result<std::sync::MutexGuard<'_,Connection>,String> { self.0.lock().map_err(|_| "Local database unavailable".into()) }
 pub fn history_enabled(&self) -> Result<bool,String> { let db=self.db()?; Ok(db.query_row("SELECT value FROM settings WHERE key='history'",[],|r|r.get::<_,String>(0)).unwrap_or_default()=="on") }
 pub fn set_history(&self, enabled: bool) -> Result<(),String> { self.db()?.execute("INSERT INTO settings(key,value) VALUES('history',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[if enabled {"on"} else {"off"}]).map_err(|_| "Could not save history setting")?; Ok(()) }
 pub fn save(&self, s: &Session) -> Result<(),String> {
  if s.id.len()>64 || s.id.is_empty() || !s.id.chars().all(|c| c.is_ascii_alphanumeric() || c=='-') || !["Meeting","Lecture","Presentation","Practice"].contains(&s.kind.as_str()) || s.transcript.len()>5_000_000 || s.notes.len()>1_000_000 || s.updated_at<0 || serde_json::from_str::<Vec<Note>>(&s.notes).is_err() { return Err("Invalid session data".into()); }
  let db=self.db()?;
  let enabled: bool=db.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key='history' AND value='on')",[],|r|r.get(0)).map_err(|_| "Could not read settings")?;
  if !enabled {return Err("Session history is disabled".into())}
  db.execute("INSERT INTO sessions(id,kind,transcript,notes,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,transcript=excluded.transcript,notes=excluded.notes,updated_at=excluded.updated_at",params![s.id,s.kind,s.transcript,s.notes,s.updated_at]).map_err(|_| "Could not save session")?; Ok(())
 }
 pub fn list(&self) -> Result<Vec<Session>,String> { let db=self.db()?; let mut q=db.prepare("SELECT id,kind,transcript,notes,updated_at FROM sessions ORDER BY updated_at DESC LIMIT 500").map_err(|_| "Could not read sessions")?; let rows=q.query_map([],|r|Ok(Session{id:r.get(0)?,kind:r.get(1)?,transcript:r.get(2)?,notes:r.get(3)?,updated_at:r.get(4)?})).map_err(|_| "Could not read sessions")?; rows.collect::<Result<Vec<_>,_>>().map_err(|_| "Could not read sessions".into()) }
 pub fn delete(&self,id:&str) -> Result<(),String> { if id.len()>64 {return Err("Invalid session ID".into())} self.db()?.execute("DELETE FROM sessions WHERE id=?1",[id]).map_err(|_| "Could not delete session")?; Ok(()) }
 pub fn delete_all(&self) -> Result<(),String> { self.db()?.execute_batch("BEGIN IMMEDIATE; DELETE FROM sessions; DELETE FROM settings; COMMIT; VACUUM;").map_err(|_| "Could not delete local data")?; Ok(()) }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Note { id: String, text: String, pinned: bool }
#[cfg(test)] mod tests {
 use super::*;
 fn store()->Store {Store::init(Connection::open_in_memory().unwrap()).unwrap()}
 fn session()->Session {Session{id:"abc-1".into(),kind:"Meeting".into(),transcript:"private".into(),notes:"[]".into(),updated_at:1}}
 #[test] fn history_is_opt_in_and_delete_all_removes_data(){let s=store();assert!(!s.history_enabled().unwrap());assert!(s.save(&session()).is_err());s.set_history(true).unwrap();s.save(&session()).unwrap();assert_eq!(s.list().unwrap().len(),1);s.delete_all().unwrap();assert!(s.list().unwrap().is_empty());assert!(!s.history_enabled().unwrap());}
 #[test] fn rejects_invalid_data_and_deletes_one(){let s=store();s.set_history(true).unwrap();let mut item=session();item.kind="invalid".into();assert!(s.save(&item).is_err());s.save(&session()).unwrap();s.delete("abc-1").unwrap();assert!(s.list().unwrap().is_empty());}
}
