import http from 'k6/http';
import { check, sleep } from 'k6';
import { SharedArray } from 'k6/data';

// 1초당 150명씩 증가 (3000 → 30000)
// 180단계 × 1초 = 180초 (3분)
function generateRampUpStages() {
  const stages = [];
  const startVUs = 3000;
  const increment = 150;

  for (let i = 0; i < 180; i++) {
    stages.push({
      duration: '1s',
      target: startVUs + (i + 1) * increment
    });
  }
  return stages;
}

// 테스트 설정
export const options = {
  // 로컬 실행 명시
  scenarios: {
    default: {
      executor: 'ramping-vus',
      startVUs: 3000,
      stages: generateRampUpStages().concat([
        // 15,000명에서 30초 유지
        { duration: '30s', target: 30000 },
        // 점진적으로 감소
        { duration: '60s', target: 0 },
      ]),
      gracefulRampDown: '10s',
    },
  },
  thresholds: {
    // 타임아웃 발생 시 테스트 중단
    http_req_duration: [
      'p(95)<10000',  // 95%의 요청이 10초 이내
      { threshold: 'p(99)<10000', abortOnFail: true }, // 99%가 10초 초과 시 중단
    ],
    http_req_failed: [
      { threshold: 'rate<0.5', abortOnFail: true }, // 에러율 50% 초과 시 중단
    ],
  },
  summaryTimeUnit: 's',
  summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(90)', 'p(95)', 'p(99)', 'p(99.9)', 'p(99.99)', 'count'],

  // HTTP 연결 최적화 (대역폭 효율 증가)
  batch: 10,  // 최대 10개 요청을 배치로 처리
  batchPerHost: 6,  // 호스트당 최대 6개 동시 연결
  noConnectionReuse: false,  // 연결 재사용 활성화 (매우 중요!)

  // HTTP 타임아웃 설정
  http: {
    timeout: '10s', // 전체 HTTP 요청 타임아웃 10초
  },
};

const BASE_URL = 'https://stress-api.nad.fun';

// 에러 로깅을 위한 배열
let errorLogs = [];

// 실제 데이터 사용
const TEST_TOKENS = [
  "0x000EC551F5868c29147433B49f16c3e598027777",
  "0x00212BFBB995D5a1d0E60dfBCD2C7DC181fc7777",
  "0x0026874E31B7ACEdD2621e1706A7143199Ab7777",
  "0x002869A8657509A23F2688856EfF5aD3D7D67777",
  "0x002D10EA518904231c86FC9EA0f1253D6d347777",
  "0x004577345a02133b5b782b259f1f7c8ebB5A7777",
  "0x0056c59D04CD107641940b45D154Aad9D1337777",
  "0x00618dA2DacCB2E3158c05d897B421f4ef027777",
  "0x0085CcE47c1339cA317Fa53Ab0e389178fCc7777",
  "0x009DbD9bA2Ff198928F55052Ec4fb01260de7777",
  "0x00af04a911628E4DE6d11D853D42e2F4B36c7777",
  "0x00c7e246498C6Da0f62970CC0B07F364A6117777",
  "0x00ce76CB5C5EeC433689630B5d08B3dDec907777",
  "0x00e5950ecbFd182857A2cf2659a047a675707777",
  "0x0127b1CEBB7bDD39C760a24639E7abdED3E07777",
  "0x0170DfaCCE2a0513C304148c598919a180C17777",
  "0x01796649a7e73Fc1baF90a648CC3e73D72BE7777",
  "0x0181d3E76A24b65360a159519DBB7800235B7777",
  "0x01D077d86C5AF202E84cEcb9e56c0feD39D37777",
  "0x01E3dEe78F13A96257791302ee241f5B61297777",
  "0x01E9Da59Eb409cF7E6933155122919Fa146F7777",
  "0x01f871817ddE9e36ED4fa04f5EC4FC6059E77777",
  "0x02243d026f377330aF64C8a0b958B6C338E27777",
  "0x023D3964b7E925a92B56178721FD2aA89fc27777",
  "0x0245Bc180d9B80C33F495c7af55b4b93e7fD7777",
  "0x02509cE3CE8Ca5a64F1e2B1A0e497F29E80a7777",
  "0x0250FDB453466001224dB5C92d8B0f6A32D57777",
  "0x025De8E707505Fb2d8Fc749Dc706c4e1E9c57777",
  "0x02Aa2217cde4132542C932e4b7ee179DbAC37777",
  "0x02e7688fB7550823F124B8f5899aD7B83CE87777",
  "0x02Ed4a7F560bDeD2b6C0febFFebf8c2CD1c47777",
  "0x0314EEbcE066889FA492baC0885813958b8B7777",
  "0x0329cB37e0170301CcbC89E4c959193b494e7777",
  "0x0333559E427B8A198b35A73e41fB57F27F287777",
  "0x03370Cb647A04f44C55b4B9A7811d27a01867777",
  "0x0337B919C6bb9A727286CEe43ce770B1829C7777",
  "0x033e3e230DD8bCdba35354410d18cea632b67777",
  "0x0344232b20255E6024E2a78171A2509729777777",
  "0x0345f480Dae620D61b185deB52d6e591B4e57777",
  "0x0348e4306b7d53Ff000EE666cd0a3eDc2DF07777",
  "0x03586b5439FC480d6BAc711de5d46dB5894c7777",
  "0x036C58B8e51559143ffbaC3F215E4d8A33b67777",
  "0x03876565411F739Bf5cFEf92723a2a50E3b67777",
  "0x038fD749e01b4d3aDC5ff8C34855Ca6Ff39C7777",
  "0x0394f400eEAB12e2526988Be18E4B89E09C07777",
  "0x039cd2da5D96D2989d615DcA392a04D6e82d7777",
  "0x03b130Fc3071f5a35De232B94F7e6fc6dfb77777",
  "0x03b6Ba043f296de9467c6De30CEb300Aa2BF7777",
  "0x03b78287185f7D7BF2E7ecF323b2eD564AC07777",
  "0x03c2C824446c60FbeCE77DCD6D2473503E4A7777",
  "0x03C4D5338D3A4b5Edd4f7Ee48885C5c50de37777",
  "0x03EB7c08dC3dB771d7b933508be073CeDD517777",
  "0x03ed46d600f2050535b538683fC439D921aA7777",
  "0x03Eee94FF0766C4c1A080349177378159c277777",
  "0x03f79dE454Bc6F9d896EB8275a0d0c3F92767777",
  "0x03FA047024CCadF796EF7d947a6D48A0457E7777",
  "0x042A5e7466d43626C20FFeE0161548002A047777",
  "0x044232FF19F115D182f2Ddadf197d7f40B267777",
  "0x0442699563ca5281D6A6654Ff5C36f1A8Bd87777",
  "0x0443EaCe3E9d026135dfb92B7bE6cCfb84807777",
  "0x04632ACc3691B6095a21B91811c7231f9b7c7777",
  "0x0478b44813F724000F72b5A2dbf7ee006A847777",
  "0x04A2E15DC32fF9B0BdaE91AC5d41d09291Dd7777",
  "0x04B97c0079732F83C3856a5070FE26eb64147777",
  "0x04c5D9f5e63333477C0eB8D1474113B1ECAb7777",
  "0x04c65748a7c095c2a18aBc5A7cfA9CeEA0E57777",
  "0x04ce27818B0D266281F5769F361BCAe0Dd847777",
  "0x04ED1fa2b9B5dd296Ae187A1b1a1f0070D837777",
  "0x04f746E8E479320027e64eE8B19d57166D427777",
  "0x04f796e610409f93E4F995515cB1048Ce2dD7777",
  "0x0526AE90bf9F3b44f80b1afD71361Cc9Ed717777",
  "0x052927DEfEb12543357b3a1EccEa99EE437c7777",
  "0x052F2c6cFC781C6eee6aEE305cF4e094a0307777",
  "0x05474EBDcA44260Fb4C23a1d0C615Ecb1e5a7777",
  "0x054b34745aFBc5FA02B517D2DE6Ec74232C37777",
  "0x05524E8E1f74B67BbFa613D03DbE9DFbf2fD7777",
  "0x0567D22FbDb2D07BF0B5c4C75D3a5186a5b67777",
  "0x058E20117182F32C637527fDb354399045FF7777",
  "0x05971680b8d932496E03507F9F2eE8fAc1F17777",
  "0x0599fb7a36FBe303F6e34C809F9a42e03B9e7777",
  "0x05B90075B76FfB46c61247478a45fC1B11C57777",
  "0x05dd557E5Ca268A35649898761B06e6565Da7777",
  "0x060Ec34643e65b57531E3a11768dd88Fdb117777",
  "0x060f66708b67bEAeb2970798b3a9ea825b307777",
  "0x061B4b90fA4ea795099490354a4de41ac3097777",
  "0x063DD2ba5B73a82aBfCd536DA1c76bB9Ef977777",
  "0x064972af8a3faFf71c9CAD851FbD3334DEa47777",
  "0x0649cFd5D1b11a5242465beA474494775e5c7777",
  "0x0655967E999Ee1e720CE21a52361CfA875707777",
  "0x066573c15b431eEBEA43dde6B52bF2B84F697777",
  "0x0671745a5Fce8A90BA844fcBeA65Caf73c237777",
  "0x0687FC686Ff1b6F23b4129d51c40d1f728e67777",
  "0x0688e3F14A231ddcfb8b181E924a80df421c7777",
  "0x069A4887180b8A9d95Ab71F38e29f59A17647777",
  "0x06ABe5DeD49f62Aa42650c91353058bD1aB17777",
  "0x0739DC0B961a6763F05B9193FA01E09e03927777",
  "0x07423f8e6f8888Cb0aa67D8D7A339E3a60307777",
  "0x075451Bd2C4BAaa3C7dAde7B1aFf7CE991417777",
  "0x07685Ff3749d8C1629010E32AC8497e347497777",
  "0x07738E3c49b946FDc8AA7fA014194c08062c7777"
];

const TEST_ACCOUNTS = [
  '0x8bB92025D506101B0725d437075C1DCDdcD99425',
  '0xE598e86C883BA1B7016726904F619B087559f08F',
  '0x6792d7992ACcC658EE8725Efc60b657A7323c54e',
  '0xb194621737C2f0E3023D0788b082a55D8f75887a',
  '0xC83fC1b8e594959fBce0CAE1C713EA8E471a3Aed',
  '0x05847495818CAf203006a9454C5170871142B29E',
  '0xD1a534AA9dB45637c3A01Bb75CEdB6840a097B8f',
  '0x8D2bC12a9cd315A303F4ff1B6694c5291a2C7473',
  '0x810AAc312b3743274976d31f24e7C6F50753d8a9',
  '0x5ce7cA7A4e1b992EfF429d5505b22eFb0a1c4763',
  '0xf9ba5F61108A001Dc91f49077C78818377C44077',
  '0x0E00dE447F5ebaa431717F644e4ec5dC95e8BB64',
  '0x6b2713592Ee6e61b02fc580D3d815F8fd1B1FfC5',
  '0x4dB11d21Fb0D4fe6579e3e3CC922346f1DFA5b79',
  '0xb00bf3031bd1D7b5CB3Cb37Bdcfb004be2BfE2A6',
  '0x8af37A2E335975D9bb0e299D4e18cb5b8AF9BB85',
  '0xc4f889e331F90A72625Dc9a488E6Cc0359361eb9',
  '0xc7FfF151e5bd01e5B6e6AB50b586B05fC199271B',
  '0xE0df3f00b2A30cC235CdCaE715a16dafA78748D7',
  '0x82aC5c66a00FAADb85190a5cE2Cf8c9DB97152E6',
  '0x58f5638e9cdC8dc6a09856D7d6AfBF7427fa9498',
  '0xef7d37d777D25c5E4aC2f86271dd9ad06C58C49B',
  '0x6c3c90186654B99Fe7C86c8001d2467474f33407',
  '0xfDFB2F150896Ae5c44bBC27F7a15Ec81aF39F0Af',
  '0xD0f759CC88E1D5BCD8dF53764D2F66E02884f0Eb',
  '0x8027C25443CDB987f76f8dfd19E5104831deDD16',
  '0x48c447d7CEb9c0138e6D73D299A4cf03be768e9b',
  '0x6e4D153A53349003954299f5f68f1a5006e1A8a9',
  '0x92F3663e592C34FDEB919D0d3bb2Ec99B2580C73',
  '0xF8efA72702286Bd2BD91A005cE041d3f46B19DC2',
  '0xBD73815c09511F330f522ad255171e69Bb9F613a',
  '0x12f46b00421ad1C09d15b259f444a3a17f78DCFE',
  '0x16754fC1327FD154B04A27606eF60A816A6a34eF',
  '0xbcB9b4eFF5D7Eb991d9c316250AEAe4411fa44dD',
  '0x5cF521c728aa59b89458C7E891DbFFFEFCC47096',
  '0x22653d7cF85a841A59570E61d9954397c436AC31',
  '0x47FBf77e0e7f1BB7f546f261798C53F520b43C21',
  '0xC7918CE090037A75d549Eb71E2Dca20698AfF851',
  '0x2c5260FfD5f18B20D65Ac98aA8D5896F24a6C042',
  '0x877474Dd0928606Ff575417c37B465099fF798f7',
  '0xa265d8FFAe243055aAFec3F72B2A367eE6d649D9',
  '0x94622528322A211BAc3D07a3E616111aF556e1eF',
  '0x26e1b828366d3a1889f08560914fD45C63Fb2B4d',
  '0xdd390B446B404B1be44c3003938b6570d42461dE',
  '0x354E62Da0C98843EA38Cc3D8a0B67CEeF82BA8fa',
  '0x5f3E1F630bd2496800DEb3C1744138311DAd47a8',
  '0x37644c08301187b1d62E289D93cB86E83121d01e',
  '0x6C57Ec35c2CBd905C4d0b4869cBF6fBb13620acB',
  '0x9924c9fAF7e97Dfe25A8f239BB76FFCc6b04380B',
  '0x5967cb57764d6f9E17196580707e67F42fe6d2a2',
  '0xA002f0b7E9c3817936886Ed86f088e4496e356a8',
  '0x03F53c05d094706ddE7BAb81Ac1Ec64C8654D220',
  '0x2f2e7c46753413C1217bFb04ad39bC6d3aaB74D7',
  '0x2A9611dC9d00575FD3655513AEb7BC057C5d4276',
  '0x41516b9871f9bA5d988699eFB1be838b25687A63',
  '0x6e1C04BD0D505456676639727e257A7052FAa643',
  '0x11a993bB3dA37585343690D44bb5592Fb4A3BbF7',
  '0x19713dA1953624B007f315De91d54b503d0E8c80',
  '0xfAF52878B1689A2aF7261E14758d770BEF552566',
  '0xf54490F356B293029799a6BdF9f21A5406B22dA8',
  '0x164B690612c7EB803763188d33BFD5D7DBF2d27E',
  '0x5b79683999c039A382Abc60eA13a7f1329D4D488',
  '0xC7Ac6DC9c7f3A2b6b8527eBFA9C2B5206e8CcB39',
  '0xEE4f90290B55896C44A22834ad4842Aaa3Ba42BA',
  '0x77Ee863861Ca1dF77776532EF2026c57f2659517',
  '0xF56911467803AF5DAa8b7bcB473Ba3900765fa6D',
  '0x4307A6047Df09e17eBA9Cd2cABab12361E0DC928',
  '0x43F96478B04760a9a486D10293D8B3666eC188B4',
  '0x5e256fb60eeb5Fe71521129C9e96f1E91435EFcd',
  '0x45F1A086aa87128d69C84b681e7E530Acd4F0aC6',
  '0x6D1d3244cDB2Afc1353Dc39A940941F35fC3be05',
  '0x533b2A7Ad38f2f0b2bf90888920e22E7a4BcE554',
  '0x71884Bb5800dBe4751bd38Beeb00d6D9E1Da8b32',
  '0xE1642770e84983FcaFeC59990bc5330643B693df',
  '0xaac2Cd90c871f66E24D0943BBe3aA6Ca9a5fE374',
  '0x995a186DA764cbefe0463aB308F1ded6efe8426c',
  '0xd4Ea6c60C7fae02C534FaA4117f159f52E45423E',
  '0x9db90F2Bb73c22a58b33027E0673714332260610',
  '0xaBC8F4780FBcDd6E1a3862bBcAF60790B96d026f',
  '0x6A0785b9464FacFfd13EAf6Dc2301e950399588a',
  '0x88b4ECa9a1BAe0A9a7a75B5B50E3540D02B2fAAE',
  '0x0f97C756CD24e9E509EA65679A09AfEF344acEA7',
  '0xdB5F91DA8c20e60e0CBE8825C96bc689565E4F31',
  '0x2D8264aB603225804cd4603C6108DEe6137CA548',
  '0x2e8Ec22B603B1700429B546327918366F07169Da',
  '0xC80c16D84cDbA84c40fCCd3Dd7b1a83585739432',
  '0x4316295E487903f8f6AEc2c09Edb5f4A312e3A50',
  '0xaa5F6e22Ed86bc13A88BED5274aD34ebb62Df893',
  '0xaB19876F370b79c4841d19477c5AEDFe39236414',
  '0x1e1C5e1d464E75256F66C2f035D0f5907A443614',
  '0x5c2d988Bf5F15D98906881dd8F58ede317F6E211',
  '0x6CE2ec37C22000b1aE725B2a0f1ee3b4Ad11B58d',
  '0x0f9C6f40F5755742323414bDa4130f9ff0332EEb',
  '0xEA77Da3649E2D72372B77d23984B6b4D54aC59f2',
  '0x7127398f2544a167eC162697314D4E4d0ddD9443',
  '0xd49c44956835F21B750B8abCC2689fd007f526f9',
  '0xd8f39F7Eb731E608aA4605C8304a6Fc09aD8c6D0',
  '0x472bCBbf1C0aD57896C20F3562eC405614c5a4Ec',
];

// 랜덤 선택 함수
function getRandomItem(array) {
  return array[Math.floor(Math.random() * array.length)];
}

// 에러 로깅 함수
function logError(endpoint, status, body, duration, errorType, expectedDuration) {
  const timestamp = new Date().toISOString();
  const error = {
    timestamp,
    endpoint,
    status,
    body: body ? body.substring(0, 200) : 'No body', // 처음 200자만 저장
    duration: duration ? Math.round(duration) : 0,
    errorType, // 'status_error', 'timeout', 'slow_response'
    expectedDuration: expectedDuration || null
  };
  errorLogs.push(error);
}

// HTTP 요청 wrapper - 자동으로 에러 로깅
function httpGetWithErrorLog(url, checks = {}, expectedDuration = null) {
  const endpoint = url.replace(BASE_URL, '');
  const res = http.get(url);

  // 기본 체크에 status 200 체크 추가
  const allChecks = Object.assign({
    'status is 200': (r) => r.status === 200
  }, checks);

  const checkResult = check(res, allChecks);

  // 에러 타입 판별
  let errorType = null;
  if (res.status !== 200) {
    errorType = 'status_error';
  } else if (expectedDuration && res.timings.duration > expectedDuration) {
    errorType = 'slow_response';
  } else if (!checkResult) {
    errorType = 'check_failed';
  }

  // 실패하거나 200이 아닌 경우 로깅
  if (!checkResult || res.status !== 200) {
    logError(endpoint, res.status, res.body, res.timings.duration, errorType, expectedDuration);
  }

  return res;
}

// API 엔드포인트별 테스트 함수들
const apiTests = {
  // 1. Token 관련 API
  getToken: () => {
    const token = getRandomItem(TEST_TOKENS);
    httpGetWithErrorLog(`${BASE_URL}/token/${token}`, {
      'token response time < 100ms': (r) => r.timings.duration < 100,
    }, 100);
  },

  getTokenMetadata: () => {
    const token = getRandomItem(TEST_TOKENS);
    httpGetWithErrorLog(`${BASE_URL}/token/metadata/${token}`, {
      'metadata response time < 50ms': (r) => r.timings.duration < 50,
    }, 50);
  },

  // 2. Account 관련 API (인증 필요한 API 제외)

  // 3. Trade 관련 API
  getSwapHistory: () => {
    const token = getRandomItem(TEST_TOKENS);
    httpGetWithErrorLog(`${BASE_URL}/trade/swap-history/${token}?page=1&limit=10`, {
      'swap history response time < 200ms': (r) => r.timings.duration < 200,
    }, 200);
  },

  getHolders: () => {
    const token = getRandomItem(TEST_TOKENS);
    httpGetWithErrorLog(`${BASE_URL}/trade/holder/${token}?page=1&limit=10`, {
      'holders response time < 200ms': (r) => r.timings.duration < 200,
    }, 200);
  },

  getMarket: () => {
    const token = getRandomItem(TEST_TOKENS);
    httpGetWithErrorLog(`${BASE_URL}/trade/market/${token}`, {
      'market response time < 50ms': (r) => r.timings.duration < 50,
    }, 50);
  },

  getMetrics: () => {
    const token = getRandomItem(TEST_TOKENS);
    httpGetWithErrorLog(`${BASE_URL}/trade/metrics/${token}?interval=1`, {
      'metrics response time < 100ms': (r) => r.timings.duration < 100,
    }, 100);
  },

  // 4. Profile 관련 API
  getProfile: () => {
    const account = getRandomItem(TEST_ACCOUNTS);
    httpGetWithErrorLog(`${BASE_URL}/profile/${account}`, {
      'profile response time < 100ms': (r) => r.timings.duration < 100,
    }, 100);
  },

  getHoldToken: () => {
    const account = getRandomItem(TEST_ACCOUNTS);
    httpGetWithErrorLog(`${BASE_URL}/profile/hold-token/${account}?page=1&limit=10`, {
      'hold token response time < 300ms': (r) => r.timings.duration < 300,
    }, 300);
  },

  getTokensCreated: () => {
    const account = getRandomItem(TEST_ACCOUNTS);
    httpGetWithErrorLog(`${BASE_URL}/profile/tokens/created/${account}?page=1&limit=10`, {
      'tokens created response time < 200ms': (r) => r.timings.duration < 200,
    }, 200);
  },

  getAccountSwapHistory: () => {
    const account = getRandomItem(TEST_ACCOUNTS);
    httpGetWithErrorLog(`${BASE_URL}/profile/swap-history/${account}?page=1&limit=10`, {
      'account swap history response time < 200ms': (r) => r.timings.duration < 200,
    }, 200);
  },

  // 5. Search API
  search: () => {
    const queries = ['pump', 'token', 'ETH', 'BTC', '0x'];
    const query = getRandomItem(queries);
    httpGetWithErrorLog(`${BASE_URL}/search/${encodeURIComponent(query)}`, {
      'search response time < 300ms': (r) => r.timings.duration < 300,
    }, 300);
  },

  // 6. Order API
  getOrderByCreationTime: () => {
    httpGetWithErrorLog(`${BASE_URL}/order/creation_time?page=1&limit=20`, {
      'order creation time response time < 150ms': (r) => r.timings.duration < 150,
    }, 150);
  },

  getOrderByMarketCap: () => {
    httpGetWithErrorLog(`${BASE_URL}/order/market_cap?page=1&limit=20`, {
      'order market cap response time < 150ms': (r) => r.timings.duration < 150,
    }, 150);
  },

  getOrderByLatestTrade: () => {
    httpGetWithErrorLog(`${BASE_URL}/order/latest_trade?page=1&limit=20`, {
      'order latest trade response time < 150ms': (r) => r.timings.duration < 150,
    }, 150);
  },

  // 7. New Event API
  getNewEvent: () => {
    httpGetWithErrorLog(`${BASE_URL}/new_event`, {
      'new event response time < 150ms': (r) => r.timings.duration < 150,
    }, 150);
  },

  // 8. Hype API
  getHypeTokens: () => {
    httpGetWithErrorLog(`${BASE_URL}/hype/token?epoch=1`, {
      'hype tokens response time < 200ms': (r) => r.timings.duration < 200,
    }, 200);
  },

  // 9. Trend API (Honor Hall of Fame)
  getTrendTokens: () => {
    httpGetWithErrorLog(`${BASE_URL}/trend`, {
      'trend tokens response time < 200ms': (r) => r.timings.duration < 200,
    }, 200);
  },

  // 10. Management API (인증 필요하므로 제외)
};

// 가중치 기반 시나리오 분포 (실제 API 경로 반영)
const scenarios = [
  // 높은 빈도 (각 6-15%)
  { weight: 0.15, test: 'getToken' },
  { weight: 0.13, test: 'getSwapHistory' },
  { weight: 0.12, test: 'getHolders' },
  { weight: 0.10, test: 'search' },
  { weight: 0.09, test: 'getProfile' },
  { weight: 0.08, test: 'getNewEvent' },
  { weight: 0.07, test: 'getOrderByMarketCap' },
  { weight: 0.06, test: 'getMetrics' },

  // 중간 빈도 (각 4-5%)
  { weight: 0.05, test: 'getHoldToken' },
  { weight: 0.05, test: 'getAccountSwapHistory' },
  { weight: 0.04, test: 'getOrderByLatestTrade' },
  { weight: 0.04, test: 'getOrderByCreationTime' },

  // 낮은 빈도 (각 0.4%)
  { weight: 0.004, test: 'getMarket' },
  { weight: 0.004, test: 'getTokenMetadata' },
  { weight: 0.004, test: 'getHypeTokens' },
  { weight: 0.004, test: 'getTrendTokens' },
  { weight: 0.004, test: 'getTokensCreated' },
];

export default function () {
  // 가중치 기반으로 API 선택
  const rand = Math.random();
  let cumulative = 0;
  
  for (const scenario of scenarios) {
    cumulative += scenario.weight;
    if (rand < cumulative) {
      apiTests[scenario.test]();
      break;
    }
  }
  
  // 실제 사용자 패턴 모방 (0.1~1초 대기)
  sleep(Math.random() * 0.9 + 0.1);
}

// 테스트 종료 후 요약
export function handleSummary(data) {
  console.log('=== 전체 API 스트레스 테스트 결과 요약 ===');
  console.log(`총 요청 수: ${data.metrics.http_reqs.values.count}`);
  console.log(`평균 응답 시간: ${data.metrics.http_req_duration.values.avg?.toFixed(2)}ms`);
  console.log(`95% 응답 시간: ${data.metrics.http_req_duration.values['p(95)']?.toFixed(2)}ms`);
  console.log(`99% 응답 시간: ${data.metrics.http_req_duration.values['p(99)']?.toFixed(2)}ms`);
  console.log(`에러율: ${(data.metrics.http_req_failed.values.rate * 100)?.toFixed(2)}%`);
  console.log(`기록된 에러 수: ${errorLogs.length}`);

  // 에러 분석
  const errorAnalysis = {
    total_errors: errorLogs.length,
    total_requests: data.metrics.http_reqs.values.count,
    error_rate: ((errorLogs.length / data.metrics.http_reqs.values.count) * 100).toFixed(2) + '%',

    // 에러 타입별 분류
    by_error_type: {},

    // HTTP 상태 코드별 분류
    by_status_code: {},

    // 엔드포인트별 에러 통계
    by_endpoint: {},

    // 가장 느린 요청 TOP 20
    slowest_requests: [],

    // 모든 에러 로그
    all_errors: errorLogs
  };

  // 에러 타입별 집계
  errorLogs.forEach(error => {
    // 에러 타입별
    if (!errorAnalysis.by_error_type[error.errorType]) {
      errorAnalysis.by_error_type[error.errorType] = {
        count: 0,
        examples: []
      };
    }
    errorAnalysis.by_error_type[error.errorType].count++;
    if (errorAnalysis.by_error_type[error.errorType].examples.length < 5) {
      errorAnalysis.by_error_type[error.errorType].examples.push({
        endpoint: error.endpoint,
        status: error.status,
        duration: error.duration,
        timestamp: error.timestamp
      });
    }

    // 상태 코드별
    if (!errorAnalysis.by_status_code[error.status]) {
      errorAnalysis.by_status_code[error.status] = {
        count: 0,
        examples: []
      };
    }
    errorAnalysis.by_status_code[error.status].count++;
    if (errorAnalysis.by_status_code[error.status].examples.length < 5) {
      errorAnalysis.by_status_code[error.status].examples.push({
        endpoint: error.endpoint,
        duration: error.duration,
        timestamp: error.timestamp
      });
    }

    // 엔드포인트별
    if (!errorAnalysis.by_endpoint[error.endpoint]) {
      errorAnalysis.by_endpoint[error.endpoint] = {
        count: 0,
        error_types: {},
        avg_duration: 0,
        max_duration: 0,
        durations: []
      };
    }
    const endpointStat = errorAnalysis.by_endpoint[error.endpoint];
    endpointStat.count++;
    endpointStat.durations.push(error.duration);
    endpointStat.max_duration = Math.max(endpointStat.max_duration, error.duration);

    if (!endpointStat.error_types[error.errorType]) {
      endpointStat.error_types[error.errorType] = 0;
    }
    endpointStat.error_types[error.errorType]++;
  });

  // 엔드포인트별 평균 계산
  Object.keys(errorAnalysis.by_endpoint).forEach(endpoint => {
    const stat = errorAnalysis.by_endpoint[endpoint];
    stat.avg_duration = Math.round(
      stat.durations.reduce((a, b) => a + b, 0) / stat.durations.length
    );
    delete stat.durations; // 메모리 절약
  });

  // 가장 느린 요청 TOP 20
  errorAnalysis.slowest_requests = errorLogs
    .sort((a, b) => b.duration - a.duration)
    .slice(0, 20)
    .map(error => ({
      endpoint: error.endpoint,
      duration: error.duration,
      status: error.status,
      errorType: error.errorType,
      timestamp: error.timestamp,
      expectedDuration: error.expectedDuration
    }));

  // 엔드포인트별 실패율 계산 및 정렬
  const endpointFailureRates = Object.entries(errorAnalysis.by_endpoint)
    .map(([endpoint, stats]) => ({
      endpoint,
      error_count: stats.count,
      avg_duration: stats.avg_duration,
      max_duration: stats.max_duration,
      error_types: stats.error_types
    }))
    .sort((a, b) => b.error_count - a.error_count);

  errorAnalysis.by_endpoint = endpointFailureRates;

  // 콘솔에 요약 출력
  console.log('\n=== 에러 분석 요약 ===');
  console.log(`\n에러 타입별:`);
  Object.entries(errorAnalysis.by_error_type).forEach(([type, data]) => {
    console.log(`  ${type}: ${data.count}건`);
  });

  console.log(`\n상태 코드별:`);
  Object.entries(errorAnalysis.by_status_code).forEach(([code, data]) => {
    console.log(`  ${code}: ${data.count}건`);
  });

  console.log(`\n실패가 많은 엔드포인트 TOP 10:`);
  endpointFailureRates.slice(0, 10).forEach((item, idx) => {
    console.log(`  ${idx + 1}. ${item.endpoint}: ${item.error_count}건 (평균: ${item.avg_duration}ms, 최대: ${item.max_duration}ms)`);
  });

  return {
    'summary.json': JSON.stringify(data, null, 2),
    'error-analysis.json': JSON.stringify(errorAnalysis, null, 2),
    'error-logs.json': JSON.stringify(errorLogs, null, 2),
  };
}