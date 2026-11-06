use anyhow::Result;

#[derive(Debug)]
pub struct ReadOnlyError;

impl std::fmt::Display for ReadOnlyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("只读连接仅允许查询，无法执行此操作")
    }
}

impl std::error::Error for ReadOnlyError {}

pub fn ensure_writable(read_only: bool) -> Result<()> {
    if read_only {
        return Err(ReadOnlyError.into());
    }
    Ok(())
}

pub fn check_read_only(read_only: bool, args: &[Vec<u8>]) -> Result<()> {
    if read_only && !is_read_only_command(args) {
        return Err(ReadOnlyError.into());
    }
    Ok(())
}

/// A fixed allowlist: commands with optional writes and unrecognized modules fail closed.
pub fn is_read_only_command(args: &[Vec<u8>]) -> bool {
    let Some(op) = args.first() else { return false };
    let op = op.to_ascii_uppercase();
    let sub = || args.get(1).map(|arg| arg.to_ascii_uppercase());
    match op.as_slice() {
        b"AUTH"
        | b"SELECT"
        | b"PING"
        | b"ECHO"
        | b"QUIT"
        | b"ASKING"
        | b"INFO"
        | b"TIME"
        | b"DBSIZE"
        | b"LASTSAVE"
        | b"ROLE"
        | b"SCAN"
        | b"KEYS"
        | b"EXISTS"
        | b"TYPE"
        | b"TTL"
        | b"PTTL"
        | b"EXPIRETIME"
        | b"PEXPIRETIME"
        | b"DUMP"
        | b"RANDOMKEY"
        | b"GET"
        | b"MGET"
        | b"GETRANGE"
        | b"SUBSTR"
        | b"STRLEN"
        | b"LCS"
        | b"GETBIT"
        | b"BITCOUNT"
        | b"BITPOS"
        | b"BITFIELD_RO"
        | b"HGET"
        | b"HMGET"
        | b"HGETALL"
        | b"HEXISTS"
        | b"HKEYS"
        | b"HVALS"
        | b"HLEN"
        | b"HSTRLEN"
        | b"HSCAN"
        | b"HRANDFIELD"
        | b"HTTL"
        | b"HPTTL"
        | b"HEXPIRETIME"
        | b"HPEXPIRETIME"
        | b"LINDEX"
        | b"LLEN"
        | b"LPOS"
        | b"LRANGE"
        | b"SCARD"
        | b"SISMEMBER"
        | b"SMISMEMBER"
        | b"SMEMBERS"
        | b"SRANDMEMBER"
        | b"SSCAN"
        | b"SDIFF"
        | b"SINTER"
        | b"SINTERCARD"
        | b"SUNION"
        | b"ZCARD"
        | b"ZCOUNT"
        | b"ZLEXCOUNT"
        | b"ZRANGE"
        | b"ZRANGEBYSCORE"
        | b"ZRANGEBYLEX"
        | b"ZRANK"
        | b"ZREVRANK"
        | b"ZREVRANGE"
        | b"ZREVRANGEBYSCORE"
        | b"ZREVRANGEBYLEX"
        | b"ZSCORE"
        | b"ZMSCORE"
        | b"ZSCAN"
        | b"ZRANDMEMBER"
        | b"ZDIFF"
        | b"ZINTER"
        | b"ZINTERCARD"
        | b"ZUNION"
        | b"XLEN"
        | b"XRANGE"
        | b"XREVRANGE"
        | b"XREAD"
        | b"XPENDING"
        | b"GEODIST"
        | b"GEOHASH"
        | b"GEOPOS"
        | b"GEOSEARCH"
        | b"GEORADIUS_RO"
        | b"GEORADIUSBYMEMBER_RO"
        | b"SORT_RO"
        | b"PFCOUNT"
        | b"JSON.GET"
        | b"JSON.MGET"
        | b"JSON.TYPE"
        | b"JSON.STRLEN"
        | b"JSON.ARRLEN"
        | b"JSON.ARRINDEX"
        | b"JSON.OBJLEN"
        | b"JSON.OBJKEYS"
        | b"JSON.RESP"
        | b"MONITOR"
        | b"SUBSCRIBE"
        | b"PSUBSCRIBE"
        | b"SSUBSCRIBE"
        | b"UNSUBSCRIBE"
        | b"PUNSUBSCRIBE"
        | b"SUNSUBSCRIBE" => true,
        b"CONFIG" => matches!(sub().as_deref(), Some(b"GET")),
        b"CLIENT" => matches!(
            sub().as_deref(),
            Some(b"LIST" | b"INFO" | b"ID" | b"GETNAME" | b"GETREDIR" | b"TRACKINGINFO")
        ),
        b"CLUSTER" => matches!(
            sub().as_deref(),
            Some(
                b"INFO"
                    | b"NODES"
                    | b"SLOTS"
                    | b"SHARDS"
                    | b"REPLICAS"
                    | b"SLAVES"
                    | b"MYID"
                    | b"MYSHARDID"
                    | b"KEYSLOT"
                    | b"COUNTKEYSINSLOT"
                    | b"GETKEYSINSLOT"
                    | b"LINKS"
                    | b"COUNT-FAILURE-REPORTS"
            )
        ),
        b"SENTINEL" => matches!(
            sub().as_deref(),
            Some(
                b"GET-MASTER-ADDR-BY-NAME"
                    | b"MASTER"
                    | b"MASTERS"
                    | b"REPLICAS"
                    | b"SLAVES"
                    | b"SENTINELS"
                    | b"CKQUORUM"
                    | b"MYID"
            )
        ),
        b"MEMORY" => matches!(
            sub().as_deref(),
            Some(b"USAGE" | b"STATS" | b"DOCTOR" | b"MALLOC-STATS")
        ),
        b"OBJECT" => matches!(
            sub().as_deref(),
            Some(b"ENCODING" | b"FREQ" | b"IDLETIME" | b"REFCOUNT")
        ),
        b"SLOWLOG" => matches!(sub().as_deref(), Some(b"GET" | b"LEN")),
        b"XINFO" => matches!(sub().as_deref(), Some(b"STREAM" | b"GROUPS" | b"CONSUMERS")),
        b"PUBSUB" => matches!(
            sub().as_deref(),
            Some(b"CHANNELS" | b"NUMSUB" | b"NUMPAT" | b"SHARDCHANNELS" | b"SHARDNUMSUB")
        ),
        b"COMMAND" => matches!(
            sub().as_deref(),
            None | Some(b"COUNT" | b"DOCS" | b"INFO" | b"LIST" | b"GETKEYS" | b"GETKEYSANDFLAGS")
        ),
        b"JSON.DEBUG" => matches!(sub().as_deref(), Some(b"MEMORY")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cli, redis::command};

    #[test]
    fn allows_queries_and_rejects_side_effects_after_cli_parsing() {
        for input in [
            "get k",
            "HGETALL k",
            "CONFIG gEt timeout",
            "CLIENT LIST",
            "CLUSTER SLOTS",
            "SENTINEL MASTERS",
            "JSON.GET k $",
            "XREAD STREAMS k 0",
            "SORT_RO k",
            "GEORADIUS_RO k 0 0 1 km",
            "SELECT 2",
            "PSUBSCRIBE *",
            "MONITOR",
            "GET SET",
        ] {
            assert!(check_read_only(true, &cli::parse(input)).is_ok(), "{input}");
        }
        for input in [
            "SET k v",
            "dEl k",
            "EXPIRE k 5",
            "FLUSHDB",
            "CONFIG SET timeout 1",
            "CLIENT KILL ID 1",
            "CLUSTER RESET",
            "SENTINEL FAILOVER master",
            "SLOWLOG RESET",
            "MEMORY PURGE",
            "GETDEL k",
            "GETEX k",
            "GETSET k v",
            "XREADGROUP GROUP g c STREAMS k >",
            "PUBLISH c v",
            "EVAL 'return 1' 0",
            "EVAL_RO 'return 1' 0",
            "EVALSHA x 0",
            "EVALSHA_RO x 0",
            "FCALL f 0",
            "FCALL_RO f 0",
            "MULTI",
            "EXEC",
            "SORT k",
            "GEORADIUS k 0 0 1 km",
            "BITFIELD k GET u8 0",
            "JSON.SET k $ 1",
            "JSON.DEBUG HELP",
            "READWRITE",
            "RESET",
            "UNKNOWN k",
            r"\x53ET k v",
            "GET\0 k",
            "CONFIG",
            "CLIENT",
        ] {
            let args = cli::parse(input);
            assert!(
                check_read_only(true, &args)
                    .unwrap_err()
                    .is::<ReadOnlyError>(),
                "{input}"
            );
            assert!(check_read_only(false, &args).is_ok(), "{input}");
        }
        assert!(!is_read_only_command(&[]));
        assert!(!is_read_only_command(&[vec![0xff]]));
        assert!(is_read_only_command(&command(&["GET", "\0STORE\0"])));
    }
}
