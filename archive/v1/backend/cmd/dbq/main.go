package main

import (
	"database/sql"
	"fmt"
	"os"
	"strings"

	_ "modernc.org/sqlite"
)

func main() {
	if len(os.Args) < 3 {
		fmt.Println("usage: dbq <dbfile> <mode> [args]")
		os.Exit(2)
	}
	db, err := sql.Open("sqlite", "file:"+os.Args[1]+"?mode=ro")
	if err != nil {
		fmt.Fprintln(os.Stderr, "open err:", err)
		os.Exit(1)
	}
	defer db.Close()

	switch os.Args[2] {
	case "run":
		var status, last, comp, model, started, errJSON string
		err := db.QueryRow(`SELECT status, last_active, COALESCE(completed_at,''), model, started_at, COALESCE(error_json,'') FROM runs WHERE id=?`, os.Args[3]).Scan(&status, &last, &comp, &model, &started, &errJSON)
		if err != nil {
			fmt.Fprintln(os.Stderr, "run err:", err)
			os.Exit(1)
		}
		fmt.Printf("status=%s model=%s\nstarted=%s\nlast_active=%s\ncompleted=%s\nerror_json=%s\n", status, model, started, last, comp, errJSON)
	case "runsql":
		// runsql <query> ... accepts 1..3 args
		rows, err := db.Query(os.Args[3])
		if err != nil {
			fmt.Fprintln(os.Stderr, "query err:", err)
			os.Exit(1)
		}
		defer rows.Close()
		cols, _ := rows.Columns()
		for rows.Next() {
			vals := make([]any, len(cols))
			ptrs := make([]any, len(cols))
			for i := range vals {
				ptrs[i] = &vals[i]
			}
			rows.Scan(ptrs...)
			for i, c := range cols {
				s := fmt.Sprint(vals[i])
				if len(s) > 500 {
					s = s[:500] + fmt.Sprintf("…(+%d)", len(s)-500)
				}
				fmt.Printf("%s=%s\n", c, s)
			}
			fmt.Println("---")
		}
	case "evts":
		// evts <runid> <n> [payloadMax]
		run, n := os.Args[3], atoi(os.Args[4])
		pmax := 2000
		if len(os.Args) > 5 {
			pmax = atoi(os.Args[5])
		}
		var maxSeq int
		db.QueryRow(`SELECT COALESCE(MAX(seq),0) FROM run_events WHERE run_id=?`, run).Scan(&maxSeq)
		from := maxSeq - n + 1
		if from < 1 {
			from = 1
		}
		rows, err := db.Query(`SELECT seq, type, length(payload), created_at, payload FROM run_events WHERE run_id=? AND seq>=? ORDER BY seq`, run, from)
		if err != nil {
			fmt.Fprintln(os.Stderr, "query err:", err)
			os.Exit(1)
		}
		defer rows.Close()
		for rows.Next() {
			var seq int
			var typ, at, p string
			var ln int
			rows.Scan(&seq, &typ, &ln, &at, &p)
			fmt.Printf("### seq=%d %s len=%d at=%s\n", seq, typ, ln, at)
			if pmax > 0 {
				if len(p) > pmax {
					fmt.Println(p[:pmax] + fmt.Sprintf("\n…[+%d chars]…\n%s", len(p)-pmax, p[len(p)-300:]))
				} else {
					fmt.Println(p)
				}
			}
		}
	case "types":
		// histogram of event types for a run
		rows, err := db.Query(`SELECT type, COUNT(*), MIN(created_at), MAX(created_at) FROM run_events WHERE run_id=? GROUP BY type ORDER BY 2 DESC`, os.Args[3])
		if err != nil {
			fmt.Fprintln(os.Stderr, "query err:", err)
			os.Exit(1)
		}
		defer rows.Close()
		for rows.Next() {
			var typ, mn, mx string
			var c int
			rows.Scan(&typ, &c, &mn, &mx)
			fmt.Printf("%-45s n=%-6d %s .. %s\n", typ, c, mn, mx)
		}
	case "evt":
		var p string
		var typ string
		if err := db.QueryRow(`SELECT type, payload FROM run_events WHERE run_id=? AND seq=?`, os.Args[3], atoi(os.Args[4])).Scan(&typ, &p); err != nil {
			fmt.Fprintln(os.Stderr, "evt err:", err)
			os.Exit(1)
		}
		fmt.Fprintln(os.Stderr, "#", typ)
		fmt.Print(p)
	case "full":
		var v string
		if err := db.QueryRow(os.Args[3]).Scan(&v); err != nil {
			fmt.Fprintln(os.Stderr, "full err:", err)
			os.Exit(1)
		}
		fmt.Print(v)
	case "sval":
		var v string
		if err := db.QueryRow(`SELECT value FROM settings WHERE key=?`, os.Args[3]).Scan(&v); err != nil {
			fmt.Fprintln(os.Stderr, "sval err:", err)
			os.Exit(1)
		}
		fmt.Print(v)
	case "sessions":
		rows, err := db.Query(`SELECT target, title, updated_at FROM sessions WHERE title LIKE '%哈尔%' ORDER BY updated_at DESC`)
		if err != nil {
			fmt.Fprintln(os.Stderr, "query err:", err)
			os.Exit(1)
		}
		defer rows.Close()
		for rows.Next() {
			var t, title, up string
			rows.Scan(&t, &title, &up)
			fmt.Printf("%s | %s | %s\n", t, title, up)
		}
	}
}

func atoi(s string) int {
	n := 0
	for _, r := range s {
		if r < '0' || r > '9' {
			break
		}
		n = n*10 + int(r-'0')
	}
	return n
}

var _ = strings.TrimSpace
