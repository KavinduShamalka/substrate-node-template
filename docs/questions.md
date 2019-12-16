## Issues & Questions:

  - When calling `submit_signed(call)`, how to control who is signing the tx?
  - When to use `submit_signed(call)` vs `sign_and_submit(call, acct)`?
  - `sign_and_submit(call, acct)` is parallel to `submit_unsigned()`?

  - Error msg when using `submit_signed()`,
    ```
    (offchain call) Error submitting a transaction to the pool: Pool(TooLowPriority { old: 30197, new: 30169 })
    ```
    - when using `submit_signed()`. how to specify the priority? Am I having duplicate call here?

  - how to use `sign_and_submit()`?
  - how critical to limit the length of the vector?
  - If same parameters in unsigned tx call, returning
    ```
    WARN txpool  (offchain call) Error submitting a transaction to the pool: Pool(TemporarilyBanned)
    ```
  - how important is the max length limit in a vector
  - How to add a real account that I am trusted to submit transaction?
  - Try run with two nodes and see the blockchain interaction
