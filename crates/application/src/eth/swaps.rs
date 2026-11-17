use std::sync::LazyLock;

use alloy_primitives::{B256, I256, U256, keccak256};

use domain::eth::{EthAddress, EthLog, PoolCandidate, PoolKey, SwapFamily};

pub static V2_SWAP: LazyLock<B256> =
    LazyLock::new(|| keccak256(b"Swap(address,uint256,uint256,uint256,uint256,address)"));

pub static V3_SWAP: LazyLock<B256> =
    LazyLock::new(|| keccak256(b"Swap(address,address,int256,int256,uint160,uint128,int24)"));

pub static V4_SWAP: LazyLock<B256> = LazyLock::new(|| {
    keccak256(b"Swap(bytes32,address,int128,int128,uint160,uint128,int24,uint24)")
});

const WORD: usize = 32;
const V2_DATA: usize = 4 * WORD;
const V3_DATA: usize = 5 * WORD;
const V4_DATA: usize = 6 * WORD;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Flow {
    gave: [U256; 2],
    got: [U256; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SwapLog {
    candidate: PoolCandidate,
    flow: Flow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Oriented {
    pub token_in: EthAddress,
    pub amount_in: U256,
    pub token_out: EthAddress,
    pub amount_out: U256,
}

impl SwapLog {
    pub fn candidate(&self) -> &PoolCandidate {
        &self.candidate
    }

    pub fn flow(&self) -> &Flow {
        &self.flow
    }
}

impl Flow {
    pub fn orient(&self, tokens: &[EthAddress]) -> Option<Oriented> {
        let [token0, token1] = tokens else {
            return None;
        };

        let net_gave = [
            self.gave[0].saturating_sub(self.got[0]),
            self.gave[1].saturating_sub(self.got[1]),
        ];
        let net_got = [
            self.got[0].saturating_sub(self.gave[0]),
            self.got[1].saturating_sub(self.gave[1]),
        ];

        match (net_gave, net_got) {
            ([gave, U256::ZERO], [U256::ZERO, got]) if gave > U256::ZERO && got > U256::ZERO => {
                Some(Oriented {
                    token_in: *token0,
                    amount_in: gave,
                    token_out: *token1,
                    amount_out: got,
                })
            }
            ([U256::ZERO, gave], [got, U256::ZERO]) if gave > U256::ZERO && got > U256::ZERO => {
                Some(Oriented {
                    token_in: *token1,
                    amount_in: gave,
                    token_out: *token0,
                    amount_out: got,
                })
            }
            _ => None,
        }
    }
}

pub fn decode_swap(log: &EthLog) -> Option<SwapLog> {
    let topics = log.topics();
    let signature = topics.first()?;

    if topics.len() != 3 {
        return None;
    }

    let data = log.data();

    if signature == &*V2_SWAP && data.len() == V2_DATA {
        return Some(SwapLog {
            candidate: PoolCandidate::new(
                PoolKey::new(*log.address(), None),
                SwapFamily::UniswapV2,
            ),
            flow: Flow {
                gave: [unsigned(data, 0)?, unsigned(data, 1)?],
                got: [unsigned(data, 2)?, unsigned(data, 3)?],
            },
        });
    }

    if signature == &*V3_SWAP && data.len() == V3_DATA {
        let (gave0, got0) = split(signed(data, 0)?);
        let (gave1, got1) = split(signed(data, 1)?);

        return Some(SwapLog {
            candidate: PoolCandidate::new(
                PoolKey::new(*log.address(), None),
                SwapFamily::UniswapV3,
            ),
            flow: Flow {
                gave: [gave0, gave1],
                got: [got0, got1],
            },
        });
    }

    if signature == &*V4_SWAP && data.len() == V4_DATA {
        let (got0, gave0) = split(signed(data, 0)?);
        let (got1, gave1) = split(signed(data, 1)?);

        return Some(SwapLog {
            candidate: PoolCandidate::new(
                PoolKey::new(*log.address(), Some(topics[1])),
                SwapFamily::UniswapV4,
            ),
            flow: Flow {
                gave: [gave0, gave1],
                got: [got0, got1],
            },
        });
    }

    None
}

fn unsigned(data: &[u8], index: usize) -> Option<U256> {
    let start = index.checked_mul(WORD)?;
    Some(U256::from_be_slice(
        data.get(start..start.checked_add(WORD)?)?,
    ))
}

fn signed(data: &[u8], index: usize) -> Option<I256> {
    let start = index.checked_mul(WORD)?;
    I256::try_from_be_slice(data.get(start..start.checked_add(WORD)?)?)
}

fn split(value: I256) -> (U256, U256) {
    let magnitude = value.unsigned_abs();

    if value.is_negative() {
        (U256::ZERO, magnitude)
    } else {
        (magnitude, U256::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::Bytes;

    use super::*;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn words(values: &[I256]) -> Bytes {
        Bytes::from(
            values
                .iter()
                .flat_map(|value| value.to_be_bytes::<32>())
                .collect::<Vec<u8>>(),
        )
    }

    fn int(value: i64) -> I256 {
        I256::try_from(value).unwrap()
    }

    fn v2_log(amounts: [i64; 4]) -> EthLog {
        EthLog::new(
            address(7),
            vec![
                *V2_SWAP,
                address(1).address().into_word(),
                address(2).address().into_word(),
            ],
            words(&amounts.map(int)),
        )
    }

    fn v3_log(amount0: i64, amount1: i64) -> EthLog {
        EthLog::new(
            address(7),
            vec![
                *V3_SWAP,
                address(1).address().into_word(),
                address(2).address().into_word(),
            ],
            words(&[int(amount0), int(amount1), int(1), int(2), int(3)]),
        )
    }

    fn v4_log(amount0: i64, amount1: i64) -> EthLog {
        EthLog::new(
            address(7),
            vec![
                *V4_SWAP,
                B256::with_last_byte(0xaa),
                address(1).address().into_word(),
            ],
            words(&[int(amount0), int(amount1), int(1), int(2), int(3), int(500)]),
        )
    }

    fn tokens() -> [EthAddress; 2] {
        [address(10), address(11)]
    }

    #[test]
    fn the_signatures_are_the_ones_uniswap_emits() {
        assert_eq!(
            format!("{:#x}", *V2_SWAP),
            "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822"
        );
        assert_eq!(
            format!("{:#x}", *V3_SWAP),
            "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67"
        );
    }

    #[test]
    fn a_v2_swap_gives_token0_and_takes_token1() {
        let decoded = decode_swap(&v2_log([100, 0, 0, 40])).unwrap();

        assert_eq!(decoded.candidate().family(), SwapFamily::UniswapV2);
        assert_eq!(decoded.candidate().key().address(), &address(7));
        assert_eq!(decoded.candidate().key().pool_id(), None);
        assert_eq!(
            decoded.flow().orient(&tokens()),
            Some(Oriented {
                token_in: address(10),
                amount_in: U256::from(100),
                token_out: address(11),
                amount_out: U256::from(40),
            })
        );
    }

    #[test]
    fn a_v2_flash_swap_is_judged_by_its_net_flow() {
        let decoded = decode_swap(&v2_log([100, 0, 30, 40])).unwrap();

        assert_eq!(
            decoded.flow().orient(&tokens()),
            Some(Oriented {
                token_in: address(10),
                amount_in: U256::from(70),
                token_out: address(11),
                amount_out: U256::from(40),
            })
        );
    }

    #[test]
    fn a_v3_positive_amount_is_what_the_pool_received() {
        let decoded = decode_swap(&v3_log(-40, 100)).unwrap();

        assert_eq!(decoded.candidate().family(), SwapFamily::UniswapV3);
        assert_eq!(
            decoded.flow().orient(&tokens()),
            Some(Oriented {
                token_in: address(11),
                amount_in: U256::from(100),
                token_out: address(10),
                amount_out: U256::from(40),
            })
        );
    }

    #[test]
    fn a_v4_negative_amount_is_what_the_swapper_paid() {
        let decoded = decode_swap(&v4_log(-100, 40)).unwrap();

        assert_eq!(decoded.candidate().family(), SwapFamily::UniswapV4);
        assert_eq!(
            decoded.candidate().key().pool_id(),
            Some(&B256::with_last_byte(0xaa))
        );
        assert_eq!(
            decoded.flow().orient(&tokens()),
            Some(Oriented {
                token_in: address(10),
                amount_in: U256::from(100),
                token_out: address(11),
                amount_out: U256::from(40),
            })
        );
    }

    #[test]
    fn a_swap_that_moves_one_side_only_has_no_direction() {
        assert!(
            decode_swap(&v3_log(100, 0))
                .unwrap()
                .flow()
                .orient(&tokens())
                .is_none()
        );
        assert!(
            decode_swap(&v3_log(100, 100))
                .unwrap()
                .flow()
                .orient(&tokens())
                .is_none()
        );
    }

    #[test]
    fn a_log_of_the_wrong_shape_decodes_to_nothing() {
        let mut short = v3_log(-1, 1);
        short = EthLog::new(
            *short.address(),
            short.topics().to_vec(),
            Bytes::from(short.data()[..short.data().len() - 1].to_vec()),
        );
        let stray = EthLog::new(address(7), vec![B256::with_last_byte(5)], Bytes::new());
        let two_topics = EthLog::new(
            address(7),
            vec![*V3_SWAP, B256::ZERO],
            v3_log(-1, 1).data().clone(),
        );

        assert!(decode_swap(&short).is_none());
        assert!(decode_swap(&stray).is_none());
        assert!(decode_swap(&two_topics).is_none());
    }

    #[test]
    fn the_extreme_amounts_do_not_overflow() {
        let extreme = EthLog::new(
            address(7),
            vec![
                *V3_SWAP,
                address(1).address().into_word(),
                address(2).address().into_word(),
            ],
            words(&[I256::MIN, I256::MAX, int(1), int(2), int(3)]),
        );

        assert!(
            decode_swap(&extreme)
                .unwrap()
                .flow()
                .orient(&tokens())
                .is_some()
        );
    }
}
