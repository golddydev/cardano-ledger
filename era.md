# Cardano Ledger Eras

This document provides a comprehensive overview of the major features and updates introduced in each era of the Cardano blockchain, from Byron to Conway.

## Overview

Cardano's development follows a structured approach organized into distinct **eras**, each named after notable figures in mathematics, computer science, and literature. Each era represents a significant upgrade to the blockchain's capabilities, building upon the foundation laid by previous eras.

| Era | Protocol Versions | Primary Focus |
|-----|------------------|---------------|
| **Byron** | 0.x - 1.x | Foundation & Launch |
| **Shelley** | 2.x - 3.x | Decentralization & Staking |
| **Allegra** | 4.x | Timelocks |
| **Mary** | 5.x | Multi-Asset Support |
| **Alonzo** | 6.x | Smart Contracts (Plutus) |
| **Babbage** | 7.x - 8.x | Script Improvements |
| **Conway** | 9.x - 10.x | On-Chain Governance |

---

## Byron Era — Foundation

**Named after:** Lord Byron, poet and father of Ada Lovelace

The Byron era established the foundation of the Cardano blockchain. It was the original implementation that enabled basic cryptocurrency functionality.

### Key Features

#### Core Blockchain Functionality
- **Mainnet Launch**: Established the initial Cardano network
- **ADA Cryptocurrency**: Native cryptocurrency for transactions and value transfer
- **UTXO Model**: Unspent Transaction Output model for tracking coin ownership
- **Preservation of Value**: Every coin in the system is accounted for, and the total amount is unchanged by every transaction

#### Wallet Infrastructure
- **Daedalus Wallet**: Full-node desktop wallet for secure ADA storage
- **Yoroi Wallet**: Light-weight browser extension wallet

#### Consensus Protocol
- **Ouroboros Classic**: Original proof-of-stake protocol
- **Federated Network**: Block production managed by founding entities (IOHK, Emurgo, Cardano Foundation)
- **Heavyweight Delegation**: Stake distribution mediated by heavyweight delegation certificates

#### Transaction Features
- **Basic Transactions**: Transfer of ADA between addresses
- **Transaction Witnesses**: Authentication of transaction data through cryptographic signatures
- **Delegation Certificates**: Validity of certificates for block-signing rights delegation

#### Update Mechanism
- **Voting System**: Identification of voters and participants for update proposals
- **Software Updates**: Mechanism for protocol parameter and software updates

#### Key Limitations
- No stake/staking rights associated with addresses
- Centralized block production
- No smart contract support
- No native token support

---

## Shelley Era — Decentralization

**Named after:** Mary Shelley, author of Frankenstein

The Shelley era marked the transition from a federated network to full decentralization, introducing the staking and delegation system that is central to Cardano's proof-of-stake mechanism.

### Key Features

#### Decentralization
- **Community-Run Stake Pools**: Transition from federated network to community-operated block production
- **Ouroboros Praos**: Enhanced proof-of-stake consensus protocol with improved security guarantees
- **Permissive BFT Transition**: Bridge mechanism to transition from Byron to Shelley

#### Staking & Delegation
- **Stake Registration**: Credentials can be registered to activate stake for protocol participation
- **Stake Delegation**: ADA holders can delegate their stake to pools without transferring custody
- **Stake Pools**: Registered entities that produce blocks on behalf of delegators
- **Pool Parameters**:
  - Pool owners, cost, margin, and pledge
  - Reward account specification
  - VRF verification key
  - Pool relays and metadata

#### Reward System
- **Monetary Expansion**: New ADA distributed from reserves
- **Reward Distribution**: Stake-based rewards for pool operators and delegators
- **Treasury System**: Portion of rewards allocated to treasury for future development
- **Fee Collection**: Transaction fees distributed to block producers

#### Certificates
- **Stake Key Registration/Deregistration**: Register or remove stake credentials
- **Delegation Certificates**: Delegate stake to a pool
- **Pool Registration/Retirement**: Register new pools or announce retirement
- **Genesis Key Delegation**: Special certificates for genesis keys
- **Move Instantaneous Rewards (MIR)**: Transfer rewards from reserves/treasury

#### Account Model
Coins are tracked in one of the following categories:
1. **Circulation (UTxO)**: Spendable outputs
2. **Deposit Pot**: Deposits for registrations
3. **Fee Pot**: Collected transaction fees
4. **Reserves**: For monetary expansion
5. **Rewards**: Account-based rewards
6. **Treasury**: Funds for future development

#### Protocol Parameters
- Configurable parameters for fees, pool limits, rewards, etc.
- Update mechanism for modifying parameters through governance

---

## Allegra Era — Timelocks

**Named after:** Allegra Byron, daughter of Lord Byron

The Allegra era introduced time-based constraints for scripts, enabling more sophisticated conditional logic in native scripts.

### Key Features

#### Timelock Scripts
- **Time-Bounded Conditions**: Scripts can specify validity based on slot numbers
- **Enhanced Multi-Signature**: Extension of multi-signature scripts with temporal constraints
- **Script Clauses**:
  - `RequireSignature`: Requires a specific key signature
  - `RequireAllOf`: All sub-clauses must be satisfied
  - `RequireAnyOf`: Any sub-clause must be satisfied
  - `RequireMOf`: M of N sub-clauses must be satisfied
  - `RequireTimeExpire`: Valid until a specific slot
  - `RequireTimeStart`: Valid from a specific slot

#### Use Cases
- **Time-Locked Funds**: Release funds only after a certain time
- **Escrow Arrangements**: Multi-party agreements with time constraints
- **Vesting Schedules**: Gradual release of tokens over time
- **Conditional Authority**: Changing signing requirements over time

#### Example
```
Before slot 1000: Either key A or key B can sign
After slot 1000: Only key A can sign
```

---

## Mary Era — Multi-Asset Support

**Named after:** Mary Shelley (continuing the Goguen theme)

The Mary era introduced native multi-asset support, allowing users to create and transact with custom tokens directly on the Cardano blockchain without requiring smart contracts.

### Key Features

#### Native Token Support
- **User-Defined Assets**: Create custom tokens directly on-chain
- **Multi-Asset Ledger**: Transactions can include multiple asset types
- **Value Bundles**: UTxO outputs contain bundles of ADA and native tokens
- **Policy Scripts**: Native scripts control minting and burning

#### Token Mechanics
- **Minting**: Create new tokens according to minting policy
- **Burning**: Destroy tokens by sending to the burn address
- **Transferring**: Send tokens between addresses just like ADA
- **Asset Identification**: Tokens identified by PolicyId + AssetName

#### MultiAsset Type
```
Value = Coin + MultiAsset
MultiAsset = Map PolicyId (Map AssetName Quantity)
```

#### Key Properties
- **No Smart Contracts Required**: Token creation uses native scripts only
- **First-Class Citizens**: Native tokens treated the same as ADA in the ledger
- **Minimum UTxO Value**: Each UTxO must contain minimum ADA (for storage costs)
- **Preservation of Value**: Total token supply tracked and verified

#### Use Cases
- Fungible Tokens (like ERC-20)
- Non-Fungible Tokens (NFTs)
- Security Tokens
- Stablecoins
- Loyalty Points

---

## Alonzo Era — Smart Contracts

**Named after:** Alonzo Church, mathematician and computer scientist

The Alonzo era introduced Plutus smart contracts, transforming Cardano into a full-featured smart contract platform with the Extended UTxO (eUTxO) model.

### Key Features

#### Plutus Smart Contracts
- **Phase-2 Scripts**: Scripts executed outside the ledger rules (vs. Phase-1 native scripts)
- **Plutus Core**: Low-level functional programming language for smart contracts
- **PlutusV1**: First version of the Plutus smart contract language
- **Deterministic Execution**: Script execution is fully deterministic

#### Extended UTxO Model (eUTxO)
- **Datum**: Additional data attached to UTxO outputs locked by scripts
- **Redeemer**: User-provided input to scripts when spending
- **Script Context**: Transaction information passed to scripts for validation
- **Validator Scripts**: Scripts that validate spending of outputs

#### Execution Units & Cost Model
- **ExUnits**: Budget expressed in memory and CPU steps
- **Cost Models**: Convert execution costs to fees (protocol parameters)
- **Deterministic Fees**: Exact costs known before submission
- **Collateral**: Inputs to cover fees in case of script failure

#### Collateral System
- **Collateral Inputs**: Separate inputs consumed only on script failure
- **Collateral Percentage**: Protocol parameter (usually 150%)
- **Protection**: Ensures failed transactions still pay fees

#### Script Types
- **Spending Scripts**: Validate spending of UTxO outputs
- **Minting Scripts**: Control token minting and burning
- **Certifying Scripts**: Validate certificates (delegation, etc.)
- **Rewarding Scripts**: Validate reward withdrawals

#### Key Properties
- **Backwards Compatibility**: All script languages supported indefinitely
- **Parallel Execution**: eUTxO enables parallel transaction processing
- **Formal Verification**: Plutus designed for formal verification

---

## Babbage Era — Script Enhancements

**Named after:** Charles Babbage, mathematician and inventor of the analytical engine

The Babbage era focused on incremental improvements to the smart contract platform, introducing reference scripts, inline datums, and other enhancements to improve developer experience and reduce costs.

### Key Features

#### Reference Inputs (CIP-31)
- **Read-Only Inputs**: Reference UTxOs without spending them
- **Shared State**: Multiple transactions can reference same data
- **Reduced Costs**: No need to recreate reference data in each transaction
- **Use Cases**: Oracles, shared configuration, on-chain registries

#### Inline Datums (CIP-32)
- **Datum in UTxO**: Store actual datum in output instead of just hash
- **Simplified Workflows**: No need to provide datum when spending
- **Reduced Transaction Size**: Avoid repeating datum in spending transaction
- **Better Composability**: Scripts can easily read data from other outputs

#### Reference Scripts (CIP-33)
- **On-Chain Script Storage**: Store scripts in UTxOs for reuse
- **Reduced Transaction Size**: Reference existing scripts instead of including them
- **Cost Savings**: Pay storage cost once, reference many times
- **Script Libraries**: Enable shared script infrastructure

#### Collateral Improvements
- **Collateral Return**: Specify change address for collateral
- **Total Collateral Field**: Explicit collateral amount in transaction
- **Better UX**: Avoid losing entire collateral UTxO on failure

#### Protocol Simplifications
- **Removed Overlay Schedule**: Simplified block production rules
- **Removed d Parameter**: Decentralization parameter no longer needed
- **Removed Extra Entropy**: Simplified VRF operations
- **Single VRF Value**: Block headers contain single VRF proof

#### PlutusV2
- **Enhanced Script Context**: More information available to scripts
- **Improved Builtins**: Additional built-in functions
- **Better Serialization**: Optimized for reference scripts

---

## Conway Era — On-Chain Governance

**Named after:** John Horton Conway, mathematician

The Conway era implements CIP-1694, introducing comprehensive on-chain governance enabling the Cardano community to participate directly in protocol decisions. This represents the **Voltaire** phase of Cardano's development roadmap.

### Key Features

#### Governance Framework (CIP-1694)
- **On-Chain Voting**: Direct voting on protocol changes and treasury
- **Transparent Decision-Making**: All governance actions recorded on-chain
- **Decentralized Control**: Community-driven protocol evolution

#### Delegated Representatives (DReps)
- **DRep Registration**: Any ADA holder can become a DRep
- **Vote Delegation**: Delegate voting power to DReps
- **DRep Expiry**: DReps must remain active to maintain status
- **Special DReps**:
  - `AlwaysAbstain`: Abstains on all votes
  - `AlwaysNoConfidence`: Votes no confidence on all proposals

#### Constitutional Committee (CC)
- **Committee Members**: Elected group ensuring constitutional compliance
- **Hot/Cold Keys**: Separation of operational and secure keys
- **Term Limits**: Committee members have defined terms
- **Thresholds**: Configurable voting thresholds for different actions

#### Governance Actions
- **Parameter Updates**: Modify protocol parameters
- **Hard Fork Initiation**: Propose protocol upgrades
- **Treasury Withdrawals**: Request funds from treasury
- **No Confidence**: Motion of no confidence in committee
- **Update Committee**: Add/remove committee members
- **New Constitution**: Propose constitutional changes
- **Info Actions**: Non-binding polls and announcements

#### Proposal Lifecycle
1. **Submission**: Submit proposal with deposit
2. **Voting Period**: DReps, SPOs, and CC vote
3. **Ratification**: Proposal passes if thresholds met
4. **Enactment**: Changes take effect after ratification
5. **Expiry**: Proposals expire if not ratified in time

#### Voting Mechanics
- **Vote Types**: Yes, No, Abstain
- **Stake-Weighted**: Votes weighted by delegated stake
- **Multiple Voter Types**: DReps, Stake Pool Operators, Constitutional Committee
- **Threshold Parameters**: Different thresholds for different action types

#### Treasury System
- **Decentralized Treasury**: Community-controlled development funds
- **Withdrawal Proposals**: Request treasury funds via governance
- **Deposit Mechanism**: Proposal deposits refunded upon completion

#### PlutusV3
- **Enhanced Script Context**: Governance information available to scripts
- **Improved Certificates**: Scripts can validate new certificate types
- **Optional Spending Datums**: CIP-0069 implementation

#### Bootstrap Phase
- **Gradual Rollout**: Restricted governance during initial phase
- **CommitteeMinSize Ignored**: Flexibility during bootstrap
- **DRep Threshold Adjustments**: Modified thresholds during transition

---

## Era Timeline and Hard Forks

| Era | Mainnet Date | Hard Fork Event |
|-----|--------------|-----------------|
| Byron | September 2017 | Genesis |
| Shelley | July 2020 | Shelley Hard Fork |
| Allegra | December 2020 | Allegra Hard Fork |
| Mary | March 2021 | Mary Hard Fork |
| Alonzo | September 2021 | Alonzo Hard Fork |
| Babbage | September 2022 | Vasil Hard Fork |
| Conway | 2024 | Chang Hard Fork |

---

## Protocol Version Mapping

| Era | Major Version | Minor Versions |
|-----|--------------|----------------|
| Byron | 0, 1 | Various |
| Shelley | 2 | 0 |
| Allegra | 3 | 0 |
| Mary | 4 | 0 |
| Alonzo | 5, 6 | 0 |
| Babbage | 7, 8 | 0 |
| Conway | 9, 10 | 0 |

---

## Further Reading

### Formal Specifications
- [Byron Chain Spec](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/byron-blockchain.pdf)
- [Byron Ledger Spec](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/byron-ledger.pdf)
- [Shelley Ledger Spec](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/shelley-ledger.pdf)
- [Shelley Delegation Design](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/shelley-delegation.pdf)
- [Mary (Multi-Asset) Ledger Spec](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/mary-ledger.pdf)
- [Alonzo (Plutus) Ledger Spec](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/alonzo-ledger.pdf)
- [Babbage Ledger Spec](https://github.com/intersectmbo/cardano-ledger/releases/latest/download/babbage-ledger.pdf)
- [Conway Formal Spec (WIP)](https://github.com/intersectmbo/formal-ledger-specifications)

### Cardano Improvement Proposals (CIPs)
- [CIP-1694: On-Chain Governance](https://github.com/cardano-foundation/CIPs/tree/master/CIP-1694)
- [CIP-31: Reference Inputs](https://github.com/cardano-foundation/CIPs/pull/159)
- [CIP-32: Inline Datums](https://github.com/cardano-foundation/CIPs/pull/160)
- [CIP-33: Reference Scripts](https://github.com/cardano-foundation/CIPs/pull/161)
- [CIP-0069: Plutus Improvements](https://github.com/cardano-foundation/CIPs/tree/master/CIP-0069)

### Research Papers
- [Ouroboros: A Provably Secure Proof-of-Stake Protocol](https://iohk.io/en/research/library/papers/ouroboros-a-provably-secure-proof-of-stake-blockchain-protocol/)
- [Ouroboros Praos](https://iohk.io/en/research/library/papers/ouroboros-praos-an-adaptively-secure-semi-synchronous-proof-of-stake-blockchain/)
- [The Extended UTXO Model](https://iohk.io/en/research/library/papers/the-extended-utxo-model/)
- [UTXOma: UTXO with Multi-Asset Support](https://iohk.io/en/research/library/papers/utxoma-utxo-with-multi-asset-support/)
- [Multi-Currency Ledgers](https://eprint.iacr.org/2020/895)

---

## Upcoming: Dijkstra Era

The next planned era after Conway is the **Dijkstra era**, which is currently in development. Early indications suggest focus areas including:
- Improved reference script cost models
- New protocol parameters for reference script handling
- PlutusV4 placeholder
- Additional governance refinements

---

*This document is maintained as part of the [cardano-ledger](https://github.com/intersectmbo/cardano-ledger) repository.*
