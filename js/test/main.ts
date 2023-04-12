import {
  createAccountAndSwapAtomic,
  createTokenSwap,
  swap,
  depositAllTokenTypes,
  withdrawAllTokenTypes,
  withdrawFees, updatePoolConfig,
} from './hyperplane-test';
import {CurveType, Numberu64} from '../src';

async function main() {
  // These test cases are designed to run sequentially and in the following order
  console.log('Run test: createTokenSwap (constant price)');
  await createTokenSwap(CurveType.ConstantPrice, new Numberu64(1));
  console.log(
    'Run test: createTokenSwap (constant product, used further in tests)',
  );
  await createTokenSwap(CurveType.ConstantProduct);
  console.log('Run test: deposit all token types');
  await depositAllTokenTypes();
  console.log('Run test: withdraw all token types');
  await withdrawAllTokenTypes();
  console.log('Run test: swap');
  await swap();
  console.log('Run test: create account, approve, swap all at once');
  await createAccountAndSwapAtomic();
  console.log('Run test: withdraw fees');
  await withdrawFees();
  console.log('Run test: update pool config');
  await updatePoolConfig();
  console.log('Success\n');
}

main()
  .catch(err => {
    console.error(err);
    process.exit(-1);
  })
  .then(() => process.exit());
