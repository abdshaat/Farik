-- What the board says the human owes (docs/SPEC.md 5.7 and 5.16): how many questions on a contract
-- are still unanswered, counted up by `question.asked` and down by `question.answered`, and whether
-- the contract waits for the human's approval, set by an escalation with reason `approval` or
-- `risk_gate` and cleared by its next move. Both are read back from the log for a database an older
-- Farik wrote, because nothing else recorded them.
ALTER TABLE task_projections ADD COLUMN open_questions INTEGER NOT NULL DEFAULT 0
    CHECK (open_questions >= 0);
ALTER TABLE task_projections ADD COLUMN awaiting_approval INTEGER NOT NULL DEFAULT 0
    CHECK (awaiting_approval IN (0, 1));
UPDATE task_projections SET open_questions = max(0,
    (SELECT count(*) FROM events
     WHERE events.task_id = task_projections.task_id AND events.kind = 'question.asked')
  - (SELECT count(*) FROM events
     WHERE events.task_id = task_projections.task_id AND events.kind = 'question.answered'));
UPDATE task_projections SET awaiting_approval = 1 WHERE EXISTS (
    SELECT 1 FROM events AS raised
    WHERE raised.task_id = task_projections.task_id
      AND raised.kind = 'escalation.raised'
      AND json_extract(raised.body, '$.reason') IN ('approval', 'risk_gate')
      AND raised.seq = (SELECT max(seq) FROM events AS last
                        WHERE last.task_id = raised.task_id AND last.kind = 'escalation.raised')
      AND NOT EXISTS (SELECT 1 FROM events AS moved
                      WHERE moved.task_id = raised.task_id AND moved.kind = 'task.transitioned'
                        AND moved.seq > raised.seq));
