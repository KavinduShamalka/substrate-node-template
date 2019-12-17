import React, { useEffect, useState } from 'react';
import { Grid } from 'semantic-ui-react';

import { useSubstrate } from './substrate-lib';
import TokenCards from './TokenCards';

function Main (props) {
  const { api } = useSubstrate();
  const [tokens, setToken] = useState({});

  useEffect(() => {
    let unsubscribe;

    api.query.priceFetch.aggPricePoints(async aggPP => {
      const tokenAggPPMap = await api.query.priceFetch.tokenAggPPMap();

      // converting vector of bytes to string
      const tokens = tokenAggPPMap[0].toArray().map(bytes =>
        Array.from(Array(bytes.length).keys()).map(ind => String.fromCharCode(bytes[ind])).join('')
      );
      const tkPriceIndices = tokenAggPPMap.toJSON()[1];

      tokens.forEach((token, ind) => {
        const lastInd = tkPriceIndices[ind].length - 1;
        const ppInd = tkPriceIndices[ind][lastInd];
        // destructuring the data type and divide by 10,000
        const price = aggPP[ppInd].toJSON()[1] / 10000;
        setToken(prev => ({ ...prev, [token]: price }));
      });
    }).then(unsub => {
      unsubscribe = unsub;
    }).catch(console.error);

    return () => unsubscribe && unsubscribe();
  }, [api.query.priceFetch]);

  return (
    <Grid.Column>
      <h1>External Price Fetch</h1>
      { Object.keys(tokens).length > 0
        ? <TokenCards tokens={ tokens }/>
        : 'No Tokens Found.' }
    </Grid.Column>

  );
}

export default function PriceFetch (props) {
  const { api } = useSubstrate();
  return (api.query.priceFetch ? <Main {...props} /> : null);
}
