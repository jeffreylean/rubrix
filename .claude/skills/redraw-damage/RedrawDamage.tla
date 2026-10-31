---------------------------- MODULE RedrawDamage ----------------------------
(***************************************************************************)
(* The editor's redraw log, `Damage` in crates/bevy_rubrix/src/damage.rs, *)
(* with the code around it that decides when a row of an editable view    *)
(* reads its spans again. Each action is one step the code takes without  *)
(* anything else running in between: an exclusive Bevy system, or one     *)
(* call inside one. SKILL.md maps every operator to its code.             *)
(***************************************************************************)
EXTENDS Integers, Sequences, TLC

CONSTANTS
    InitLines,  \* lines of the doc when it is spawned
    MaxLines,   \* the most lines the doc may hold
    Slots,      \* fynix's MAX_SLOTS, BUILT_MAX in damage.rs
    Frames,     \* frames to run
    Notes,      \* Damage::note calls to run, doc and marks together
    MaxEdits    \* the most line edits one change makes

\* usize::MAX, the index VirtualList::splice gives a slot whose row it
\* removed.
Gone == 1000

VARIABLES
    doc,      \* per line: <<seq of the note that wrote it, edit, line>>
    marksAt,  \* per line number: the Marks seq that last set its marks
    dd,       \* EditDoc.damage
    md,       \* Marks.damage, on the view
    count,    \* VirtualList.count
    slots,    \* VirtualList.slots[_].index
    rows,     \* per slot: what its row read, as fynix's Mount keeps it
    phase,    \* where the frame is: "apply", "update" or "post"
    frame,
    notes,
    mounted   \* the last step was a Mount

vars == <<doc, marksAt, dd, md, count, slots, rows, phase, frame, notes,
          mounted>>

Min(a, b) == IF a < b THEN a ELSE b
Max(a, b) == IF a > b THEN a ELSE b

\* The sequence `s` as a plain tuple. TLC 1.7.4 fails to write a lazy
\* function to its state queue when a VIEW leaves it unfingerprinted.
Tuple(s) == SelectSeq(s, LAMBDA x : TRUE)

-----------------------------------------------------------------------------
(* damage.rs: Shift, merged, Damage. Ranges are <<start, end>>.          *)

\* Shift::line from the walk state `st`: the moved line and the state
\* after.
RECURSIVE ShiftLine(_, _, _, _)
ShiftLine(edits, st, at, end) ==
    IF st.next > Len(edits) THEN <<at + st.delta, st>>
    ELSE LET e == edits[st.next]
             first == e.first - st.delta
         IN IF at >= first + e.removed
            THEN ShiftLine(edits,
                           [next |-> st.next + 1,
                            delta |-> st.delta + e.inserted - e.removed],
                           at, end)
            ELSE IF end = "Start" /\ at >= first THEN <<e.first, st>>
            ELSE IF end = "Stop" /\ at > first
                 THEN <<e.first + e.inserted, st>>
            ELSE <<at + st.delta, st>>

\* Shift::range over each range in order, in one walk.
RECURSIVE ShiftRanges(_, _, _, _)
ShiftRanges(ranges, edits, i, st) ==
    IF i > Len(ranges) THEN <<>>
    ELSE LET s == ShiftLine(edits, st, ranges[i][1], "Start")
             e == ShiftLine(edits, s[2], ranges[i][2], "Stop")
         IN <<<<s[1], e[1]>>>> \o ShiftRanges(ranges, edits, i + 1, e[2])

RECURSIVE MergeFrom(_, _, _)
MergeFrom(ranges, i, out) ==
    IF i > Len(ranges) THEN out
    ELSE LET r == ranges[i]
             last == Len(out)
         IN IF r[1] >= r[2] THEN MergeFrom(ranges, i + 1, out)
            ELSE IF last > 0 /\ r[1] <= out[last][2]
                 THEN MergeFrom(ranges, i + 1,
                        [out EXCEPT ![last] =
                            <<out[last][1], Max(out[last][2], r[2])>>])
            ELSE MergeFrom(ranges, i + 1, Append(out, r))

Merged(ranges) == MergeFrom(ranges, 1, <<>>)

RECURSIVE InsertByStart(_, _)
InsertByStart(r, sorted) ==
    IF sorted = <<>> THEN <<r>>
    ELSE IF r[1] <= Head(sorted)[1] THEN <<r>> \o sorted
    ELSE <<Head(sorted)>> \o InsertByStart(r, Tail(sorted))

RECURSIVE SortByStart(_)
SortByStart(ranges) ==
    IF ranges = <<>> THEN <<>>
    ELSE InsertByStart(Head(ranges), SortByStart(Tail(ranges)))

EditRanges(edits) ==
    [i \in 1..Len(edits) |->
        <<edits[i].first, edits[i].first + edits[i].inserted>>]

\* Damage::note.
Note(d, edits, lines) ==
    LET kept ==
            IF edits = <<>> THEN d.log
            ELSE [i \in 1..Len(d.log) |->
                    [seq |-> d.log[i].seq,
                     lines |-> Merged(ShiftRanges(d.log[i].lines, edits, 1,
                                                  [next |-> 1, delta |-> 0]))]]
        dirty == SortByStart(EditRanges(edits) \o lines)
    IN [d EXCEPT !.seq = d.seq + 1,
                 !.log = Append(kept, [seq |-> d.seq + 1,
                                       lines |-> Merged(dirty)])]

\* log.partition_point(|entry| entry.seq <= since)
RECURSIVE NotAfter(_, _, _)
NotAfter(log, since, i) ==
    IF i <= Len(log) /\ log[i].seq <= since
    THEN NotAfter(log, since, i + 1) ELSE i - 1

\* Damage::settle.
Settle(d) ==
    LET since == d.began
        old == NotAfter(d.log, since, 1)
    IN [seq |-> d.seq, began |-> d.seq, since |-> since,
        log |-> SubSeq(d.log, old + 1, Len(d.log))]

\* lines.partition_point(|lines| lines.end <= line)
RECURSIVE EndsBy(_, _, _)
EndsBy(lines, line, i) ==
    IF i <= Len(lines) /\ lines[i][2] <= line
    THEN EndsBy(lines, line, i + 1) ELSE i - 1

Covers(lines, line) ==
    LET i == EndsBy(lines, line, 1)
    IN i < Len(lines) /\ lines[i + 1][1] <= line

\* log.iter().rev().take_while(|entry| entry.seq > read).any(..)
RECURSIVE NewerCovers(_, _, _, _)
NewerCovers(log, i, line, read) ==
    IF i = 0 THEN FALSE
    ELSE IF log[i].seq > read
         THEN Covers(log[i].lines, line) \/ NewerCovers(log, i - 1, line, read)
    ELSE FALSE

\* Damage::fires.
Fires(d, line, read) ==
    read < d.since \/ NewerCovers(d.log, Len(d.log), line, read)

NoDamage == [seq |-> 0, began |-> 0, since |-> 0, log |-> <<>>]

-----------------------------------------------------------------------------
(* damage.rs row_edits, row_splices, splice_rows, and fynix's            *)
(* VirtualList::splice on the slots.                                     *)

RECURSIVE RowSplices(_, _)
RowSplices(edits, i) ==
    IF i > Len(edits) THEN <<>>
    ELSE LET e == edits[i]
             row == [at |-> e.first + 1, removed |-> e.removed - 1,
                     inserted |-> e.inserted - 1]
             replaces == e.removed > 1 \/ e.inserted > 1
             counts == row.removed # 1 \/ row.inserted # 1
         IN (IF replaces /\ counts THEN <<row>> ELSE <<>>)
            \o RowSplices(edits, i + 1)

\* VirtualList::splice, for the slots.
SpliceSlots(sl, at, removed, inserted) ==
    LET end == at + removed
        Moved(v) == IF v >= end THEN v - removed + inserted ELSE Min(v, at)
    IN Tuple([p \in 1..Len(sl) |->
                 IF at <= sl[p] /\ sl[p] < end THEN Gone
                 ELSE IF sl[p] # Gone THEN Moved(sl[p])
                 ELSE Gone])

RECURSIVE ApplySplices(_, _, _, _)
ApplySplices(sl, cnt, splices, i) ==
    IF i > Len(splices) THEN <<sl, cnt>>
    ELSE LET s == splices[i]
         IN IF Assert(s.at + s.removed <= cnt, "a splice past the list")
            THEN ApplySplices(SpliceSlots(sl, s.at, s.removed, s.inserted),
                              cnt - s.removed + s.inserted, splices, i + 1)
            ELSE <<sl, cnt>>

RECURSIVE RowsBefore(_, _, _)
RowsBefore(edits, i, n) ==
    IF i > Len(edits) THEN n
    ELSE RowsBefore(edits, i + 1, n + edits[i].removed - edits[i].inserted)

\* splice_rows: a list whose count is not the doc's rows before the
\* change is left as it is.
SpliceRows(sl, cnt, edits, after) ==
    IF cnt # RowsBefore(edits, 1, after) THEN <<sl, cnt>>
    ELSE ApplySplices(sl, cnt, RowSplices(edits, 1), 1)

-----------------------------------------------------------------------------
(* The doc's lines.                                                      *)

\* What a row of line `i` draws: the line's text and runs, and the marks
\* on its line number. EditDoc::line is None past the last line.
Content(i) ==
    IF i < Len(doc) THEN <<doc[i + 1], marksAt[i]>> ELSE <<"none">>

\* One LineEdit on the lines, its inserted lines new.
ApplyEdit(lines, e, seq, k) ==
    SubSeq(lines, 1, e.first)
    \o [j \in 1..e.inserted |-> <<seq, k, e.first + j - 1>>]
    \o SubSeq(lines, e.first + e.removed + 1, Len(lines))

RECURSIVE ApplyEdits(_, _, _, _)
ApplyEdits(lines, edits, seq, k) ==
    IF k > Len(edits) THEN lines
    ELSE ApplyEdits(ApplyEdit(lines, edits[k], seq, k), edits, seq, k + 1)

\* The lines a lex found changed runs on, new.
Relex(lines, lexed, seq) ==
    [i \in 1..Len(lines) |->
        IF i - 1 \in lexed THEN <<seq, 0, i - 1>> ELSE lines[i]]

RECURSIVE UnitRanges(_, _)
UnitRanges(set, n) ==
    IF n = 0 THEN <<>>
    ELSE UnitRanges(set, n - 1)
         \o (IF n - 1 \in set THEN <<<<n - 1, n>>>> ELSE <<>>)

\* Every edit removes at least one line and inserts at least one
\* (Change::line_edits).
After(len, e) == len - e.removed + e.inserted

EditsOn(len) ==
    {e \in [first : 0..len - 1, removed : 1..len, inserted : 1..MaxLines] :
        e.first + e.removed <= len /\ After(len, e) <= MaxLines}

\* Change::line_edits, ascending in sequential coordinates:
\* edits[i].first + edits[i].inserted <= edits[i + 1].first.
Changes ==
    {<<e>> : e \in EditsOn(Len(doc))}
    \cup
    (IF MaxEdits < 2 THEN {}
     ELSE UNION {{<<e1, e2>> :
                     e2 \in {e \in EditsOn(After(Len(doc), e1)) :
                               e.first >= e1.first + e1.inserted}} :
                 e1 \in EditsOn(Len(doc))})

-----------------------------------------------------------------------------
(* Steps.                                                                 *)

Init ==
    /\ doc = [i \in 1..InitLines |-> <<0, 0, i - 1>>]
    /\ marksAt = [l \in 0..MaxLines - 1 |-> 0]
    /\ dd = NoDamage
    /\ md = NoDamage
    /\ count = InitLines
    /\ slots = <<>>
    /\ rows = <<>>
    /\ phase = "post"
    /\ frame = 0
    /\ notes = 0
    /\ mounted = FALSE

\* frame::apply, first: settle every doc's and marks' damage.
StartFrame ==
    /\ phase = "post"
    /\ frame < Frames
    /\ dd' = Settle(dd)
    /\ md' = Settle(md)
    /\ phase' = "apply"
    /\ frame' = frame + 1
    /\ mounted' = FALSE
    /\ UNCHANGED <<doc, marksAt, count, slots, rows, notes>>

\* Tx::transact: EditDoc::run edits, lexes and notes, then
\* EditView::splice splices the view's list, in frame::apply.
Change ==
    /\ phase = "apply"
    /\ notes < Notes
    /\ \E edits \in Changes :
        LET seq == dd.seq + 1
            edited == ApplyEdits(doc, edits, seq, 1)
            spliced == SpliceRows(slots, count, edits, Len(edited))
        IN \E lexed \in SUBSET (0..Len(edited) - 1) :
            /\ doc' = Relex(edited, lexed, seq)
            /\ dd' = Note(dd, edits, UnitRanges(lexed, Len(edited)))
            /\ slots' = spliced[1]
            /\ count' = spliced[2]
    /\ notes' = notes + 1
    /\ mounted' = FALSE
    /\ UNCHANGED <<marksAt, md, rows, phase, frame>>

\* EditDoc::catch_up, at the end of frame::apply and in diff_edit's
\* catch_up: notes the lines whose runs changed, with no line edits.
CatchUp ==
    /\ phase = "apply"
    /\ notes < Notes
    /\ \E lexed \in SUBSET (0..Len(doc) - 1) \ {{}} :
        /\ doc' = Relex(doc, lexed, dd.seq + 1)
        /\ dd' = Note(dd, <<>>, UnitRanges(lexed, Len(doc)))
    /\ notes' = notes + 1
    /\ mounted' = FALSE
    /\ UNCHANGED <<marksAt, md, count, slots, rows, phase, frame>>

\* Marks::set, from any system in any schedule: notes the lines of the
\* layer's old and new marks.
SetMarks ==
    /\ notes < Notes
    /\ \E lines \in SUBSET (0..MaxLines - 1) \ {{}} :
        /\ marksAt' = [l \in 0..MaxLines - 1 |->
                          IF l \in lines THEN md.seq + 1 ELSE marksAt[l]]
        /\ md' = Note(md, <<>>, UnitRanges(lines, MaxLines))
    /\ notes' = notes + 1
    /\ mounted' = FALSE
    /\ UNCHANGED <<doc, dd, count, slots, rows, phase, frame>>

\* fynix's read_sources then place_rows, in Update before Mount:
\* VirtualList::fill places the slots over the rows near the view.
Fill ==
    /\ phase = "apply"
    /\ \E first \in 0..Len(doc) - 1, need \in 1..Slots :
        /\ first + need <= Len(doc)
        /\ LET cnt == Len(doc)
               pool == Min(Min(IF Len(slots) > 2 * need THEN need
                               ELSE Max(Len(slots), need), cnt), Slots)
               start == Min(first, cnt - pool)
               kept == SubSeq(slots, 1, Min(Len(slots), pool))
               InWindow(p) ==
                   kept[p] # Gone /\ kept[p] >= start
                   /\ kept[p] - start < pool
               Keeps(p) ==
                   InWindow(p)
                   /\ \A q \in 1..p - 1 : ~(InWindow(q) /\ kept[q] = kept[p])
               free == SelectSeq([p \in 1..pool |-> p],
                                 LAMBDA p : p > Len(kept) \/ ~Keeps(p))
               missing == SelectSeq([k \in 1..pool |-> start + k - 1],
                                    LAMBDA v : ~\E p \in 1..Len(kept) :
                                                  Keeps(p) /\ kept[p] = v)
           IN slots' = Tuple([p \in 1..pool |->
                           IF p <= Len(kept) /\ Keeps(p) THEN kept[p]
                           ELSE missing[CHOOSE j \in 1..Len(free) :
                                            free[j] = p]])
    /\ count' = Len(doc)
    /\ phase' = "update"
    /\ mounted' = FALSE
    /\ UNCHANGED <<doc, marksAt, dd, md, rows, frame, notes>>

\* EditFeed::reread's stamp.
Now == [doc |-> dd.seq, marks |-> md.seq]

\* A row built at Mount reads at once.
Built(index) == [seen |-> index, read |-> Now, shown |-> Content(index)]

\* rows.rs `line`: the slot moved by RowSlot::index's check, which
\* compares the index with the one it saw last; or EditFeed::reread
\* fires on the doc's or the marks' damage.
Checked(row, index) ==
    LET moved == index # row.seen
        fires == Fires(dd, index, row.read.doc)
                 \/ Fires(md, index, row.read.marks)
    IN [seen |-> index, read |-> Now,
        shown |-> IF moved \/ fires THEN Content(index) ELSE row.shown]

\* fynix's Mount, an exclusive system in Update: update_structure builds
\* a row for each new slot and drops the rows of slots fill truncated,
\* then update_elements checks every row.
Mount ==
    /\ phase = "update"
    /\ rows' = Tuple([p \in 1..Len(slots) |->
                   IF p <= Len(rows) THEN Checked(rows[p], slots[p])
                   ELSE Built(slots[p])])
    /\ phase' = "post"
    /\ mounted' = TRUE
    /\ UNCHANGED <<doc, marksAt, dd, md, count, slots, frame, notes>>

\* A frame whose Mount does not run.
SkipMount ==
    /\ phase = "update"
    /\ phase' = "post"
    /\ mounted' = FALSE
    /\ UNCHANGED <<doc, marksAt, dd, md, count, slots, rows, frame, notes>>

Next == StartFrame \/ Change \/ CatchUp \/ SetMarks \/ Fill \/ Mount
        \/ SkipMount

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* Properties.                                                            *)

\* splice_rows never meets a list whose count is not the doc's rows.
ListCountIsDocRows == count = Len(doc)

\* Damage's promise: for every generation a row could have read at, a
\* line whose spans changed after it fires. doc[i + 1][1] and marksAt[l]
\* are the seq of the note that last changed them.
DamageSound ==
    /\ \A i \in 0..Len(doc) - 1, read \in 0..dd.seq :
           doc[i + 1][1] > read => Fires(dd, i, read)
    /\ \A l \in 0..MaxLines - 1, read \in 0..md.seq :
           marksAt[l] > read => Fires(md, l, read)

\* What DamageSound reads and what the steps that write it read. The
\* damage check fingerprints states by it, so the slots and rows, which
\* never feed back into the doc or the damage, do not multiply them.
DamageView == <<doc, marksAt, dd, md, phase, frame, notes>>

\* After each Mount, every row shows what its line holds now.
RowsFresh ==
    mounted => \A p \in 1..Len(rows) : rows[p].shown = Content(slots[p])

=============================================================================
