#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod dao {
    use ink::storage::Mapping;
    use scale::{Decode, Encode};

    use ink::env::{
        call::{build_call, ExecutionInput, Selector},
        DefaultEnvironment,
    };

    #[derive(Encode, Decode)]
    #[cfg_attr(feature = "std", derive(Debug, PartialEq, Eq, scale_info::TypeInfo))]
    pub enum VoteType {
        Against,
        For,
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum DaoError {
        AmountShouldNotBeZero,
        AmountShouldNotExceedTheBalance,
        DurationError,
        QuorumNotReached,
        ProposalNotAccepted,
        ProposalNotFound,
        ProposalAlreadyExecuted,
        VotePeriodEnded,
        AlreadyVoted,
        TransferFailed,
    }

    #[derive(Encode, Decode)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink::storage::traits::StorageLayout
        )
    )]
    pub struct Proposal {
        to: AccountId,
        amount: Balance,
        vote_start: u64,
        vote_end: u64,
        executed: bool,
    }

    #[derive(Encode, Decode, Default)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink::storage::traits::StorageLayout
        )
    )]
    pub struct ProposalVote {
        for_votes: u64,
        against_vote: u64,
    }

    pub type ProposalId = u64;

    #[ink(storage)]
    pub struct Governor {
        proposals: Mapping<ProposalId, Proposal>,
        proposal_votes: Mapping<Proposal, ProposalVote>,
        votes: Mapping<(ProposalId, AccountId), ()>,
        next_proposal_id: ProposalId,
        quorum: u64,
        governance_token: AccountId,
    }

    impl Governor {
        #[ink(constructor, payable)]
        pub fn new(governance_token: AccountId, quorum: u64) -> Self {
            Self {
                proposals: Mapping::default(),
                proposal_votes: Mapping::default(),
                votes: Mapping::default(),
                next_proposal_id: ProposalId::default(),
                quorum,
                governance_token,
            }
        }

        #[ink(message)]
        pub fn propose(
            &mut self,
            to: AccountId,
            amount: Balance,
            duration: u64,
        ) -> Result<(), DaoError> {
            if amount == 0 {
                return Err(DaoError::AmountShouldNotBeZero);
            }

            if amount > self.env().balance() {
                return Err(DaoError::AmountShouldNotExceedTheBalance);
            }

            if duration == 0 {
                return Err(DaoError::DurationError);
            }

            let time = self.env().block_timestamp();
            let proposal = Proposal {
                to: to,
                amount: amount,
                vote_start: time,
                vote_end: (time + duration * 60),
                executed: false,
            };

            self.next_proposal_id += 1;
            self.proposals.insert(self.next_proposal_id, &proposal);

            Ok(())
        }

        #[ink(message)]
        pub fn vote(&mut self, proposal_id: ProposalId, vote: VoteType) -> Result<(), DaoError> {
            let proposal = match self.proposals.get(proposal_id) {
                Some(value) => value,
                None => return Err(DaoError::ProposalNotFound),
            };

            if proposal.executed {
                return Err(DaoError::ProposalAlreadyExecuted);
            }

            let current_time = self.env().block_timestamp();
            if current_time > proposal.vote_end {
                return Err(DaoError::VotePeriodEnded);
            }

            let caller = self.env().caller();
            if self.votes.contains((proposal_id, caller)) {
                return Err(DaoError::AlreadyVoted);
            }

            self.votes.insert((proposal_id, caller), &());

            let weight = build_call::<DefaultEnvironment>()
                .call(self.governance_token)
                .gas_limit(5000000000)
                .exec_input(
                    ExecutionInput::new(Selector::new(ink::selector_bytes!("weight")))
                        .push_arg(caller),
                )
                .returns::<u64>()
                .invoke();

            let proposal_vote = match self.proposal_votes.get(&proposal) {
                Some(votes) => match vote {
                    VoteType::Against => ProposalVote {
                        against_vote: votes.against_vote + weight,
                        for_votes: votes.for_votes,
                    },
                    VoteType::For => ProposalVote {
                        against_vote: votes.against_vote,
                        for_votes: votes.for_votes + weight,
                    },
                },
                None => match vote {
                    VoteType::Against => ProposalVote {
                        against_vote: weight,
                        for_votes: 0,
                    },
                    VoteType::For => ProposalVote {
                        against_vote: 0,
                        for_votes: weight,
                    },
                },
            };

            self.proposal_votes.insert(proposal, &proposal_vote);

            Ok(())
        }

        #[ink(message)]
        pub fn execute(&mut self, proposal_id: ProposalId) -> Result<(), DaoError> {
            let mut proposal = match self.proposals.get(&proposal_id) {
                Some(value) => value,
                None => return Err(DaoError::ProposalNotFound),
            };

            if proposal.executed {
                return Err(DaoError::ProposalAlreadyExecuted);
            }

            match self.proposal_votes.get(&proposal) {
                Some(proposal_votes) => {
                    if self.quorum > (proposal_votes.for_votes + proposal_votes.against_vote) {
                        return Err(DaoError::QuorumNotReached);
                    }

                    if proposal_votes.for_votes < proposal_votes.against_vote {
                        return Err(DaoError::ProposalNotAccepted);
                    }
                }
                None => {
                    return Err(DaoError::QuorumNotReached);
                }
            }

            proposal.executed = true;
            self.proposals.insert(proposal_id, &proposal);

            if let Err(_) = self.env().transfer(proposal.to, proposal.amount) {
                return Err(DaoError::TransferFailed);
            }

            Ok(())
        }

        // used for test
        #[ink(message)]
        pub fn now(&self) -> u64 {
            self.env().block_timestamp()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn create_contract(initial_balance: Balance) -> Governor {
            let accounts = default_accounts();
            set_sender(accounts.alice);
            set_balance(contract_id(), initial_balance);
            Governor::new(AccountId::from([0x01; 32]), 50)
        }

        fn contract_id() -> AccountId {
            ink::env::test::callee::<ink::env::DefaultEnvironment>()
        }

        fn default_accounts() -> ink::env::test::DefaultAccounts<ink::env::DefaultEnvironment> {
            ink::env::test::default_accounts::<ink::env::DefaultEnvironment>()
        }

        fn set_sender(sender: AccountId) {
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(sender);
        }

        fn set_balance(account_id: AccountId, balance: Balance) {
            ink::env::test::set_account_balance::<ink::env::DefaultEnvironment>(account_id, balance)
        }

        fn get_balance(account_id: AccountId) -> Balance {
            ink::env::test::get_account_balance::<ink::env::DefaultEnvironment>(account_id).unwrap_or_default()
        }

        #[ink::test]
        fn propose_works() {
            let accounts = default_accounts();
            let mut governor = create_contract(1000);

            assert_eq!(
                governor.propose(accounts.django, 0, 1),
                Err(DaoError::AmountShouldNotBeZero)
            );

            assert_eq!(
                governor.propose(accounts.django, 1001, 1),
                Err(DaoError::AmountShouldNotExceedTheBalance)
            );

            assert_eq!(
                governor.propose(accounts.django, 100, 0),
                Err(DaoError::DurationError)
            );

            let result = governor.propose(accounts.django, 100, 1);
            assert_eq!(result, Ok(()));

            // let proposal = governor.get_proposal(0).unwrap();
            let proposal = governor.proposals.get(1).unwrap();
            let now = governor.now();

            assert_eq!(
                proposal,
                Proposal {
                    to: accounts.django,
                    amount: 100,
                    vote_start: 0,
                    vote_end: now + 1 * 60, //ONE_MINUTE,
                    executed: false,
                }
            );

            // assert_eq!(governor.next_proposal_id(), 1);
            assert_eq!(governor.next_proposal_id, 1);
        }

        #[ink::test]
        fn quorum_not_reached() {
            let mut governor = create_contract(1000);
            let result = governor.propose(AccountId::from([0x02; 32]), 100, 1);
            assert_eq!(result, Ok(()));

            let execute = governor.execute(1);
            assert_eq!(execute, Err(DaoError::QuorumNotReached));
        }

        #[ink::test]
        fn execute_works() {
            let accounts = default_accounts();
            let mut governor = create_contract(1000);

            let result = governor.propose(accounts.eve, 100, 100);
            assert_eq!(result, Ok(()));

            let proposal = governor.proposals.get(1).unwrap();
            
            let proposal_vote = ProposalVote {
                against_vote: 29,
                for_votes: 35,
            };
            
            governor.proposal_votes.insert(proposal, &proposal_vote);
            
            let result = governor.execute(1);
            assert_eq!(result, Ok(()));
            
            let proposal = governor.proposals.get(1).unwrap();
            assert!(proposal.executed);
            
            assert_eq!(get_balance(contract_id()), 900);
        }
    }
}
